use std::path::Path;

use code_language_core::{CodeLanguageCore, ParsedSourceFile};

use crate::output::markdown::build_markdown;
use crate::output::mermaid::build_mermaid;
use crate::output::plantuml::build_plantuml;
use crate::output::text::CandidateDisplay;

pub struct ScanArgs {
    pub dir: String,
    pub format: String,
    pub output: Option<String>,
    pub depth: usize,
    pub include: String,
    pub verbose: bool,
}

/// `scan` コマンド: ローカルソースコードを読み込んでアーキテクチャを逆解析する。
pub fn run(args: ScanArgs) -> Result<(), String> {
    let dir = Path::new(&args.dir);
    if !dir.exists() {
        return Err(format!("directory not found: {}", dir.display()));
    }

    // ソースファイルを収集
    let files = collect_files(dir, &args.include, args.depth, args.verbose)?;
    if files.is_empty() {
        return Err(format!(
            "no files matched '{}' under '{}'",
            args.include,
            dir.display()
        ));
    }

    if args.verbose {
        eprintln!("[arch-gen] scan: {} file(s) found", files.len());
    }

    // CodeLanguageCore でパース → CodeIr
    let core = CodeLanguageCore::default();
    let _code_ir = core.parse_sources(&files);

    // ReverseArchitectureReasoner で ArchitectureGraph を推定
    let graph = core.reverse_architecture(&files);

    if args.verbose {
        eprintln!(
            "[arch-gen] scan: {} nodes, {} edges inferred",
            graph.nodes.len(),
            graph.edges.len()
        );
    }

    if graph.nodes.is_empty() {
        return Err(
            "no architecture components could be inferred from the source files".to_string()
        );
    }

    // quality スコアを計算（CodeLanguageCore のユーティリティを使用）
    let quality = core.evaluate_generation_quality(&graph);

    // component_names / dependency_pairs を構築
    let component_names: Vec<String> = graph.nodes.iter().map(|n| n.name.clone()).collect();
    let node_id_to_name: std::collections::HashMap<u64, &str> = graph
        .nodes
        .iter()
        .map(|n| (n.id, n.name.as_str()))
        .collect();
    let dependency_pairs: Vec<(String, String)> = graph
        .dependency_edges()
        .filter_map(|e| {
            let from = node_id_to_name.get(&e.from).map(|s| s.to_string())?;
            let to = node_id_to_name.get(&e.to).map(|s| s.to_string())?;
            Some((from, to))
        })
        .collect();

    // 出力
    let content = match args.format.as_str() {
        "mermaid" => build_mermaid(&component_names, &dependency_pairs),
        "plantuml" => build_plantuml(&component_names, &dependency_pairs),
        "json" => build_scan_json(dir, files.len(), &component_names, &dependency_pairs, quality)?,
        "markdown" => {
            let display = scan_to_candidate_display(&component_names, &dependency_pairs, quality);
            build_markdown(
                &format!("scan: {}", dir.display()),
                files.len(),
                &[display],
            )
        }
        _ => build_scan_text(dir, files.len(), &component_names, &dependency_pairs, quality, &graph),
    };

    match args.output.as_deref() {
        Some(out_path) => {
            let out = Path::new(out_path);
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("failed to create output dir: {e}"))?;
            }
            std::fs::write(out, &content)
                .map_err(|e| format!("failed to write '{}': {e}", out.display()))?;
            eprintln!("[arch-gen] scan result saved to {}", out.display());
        }
        None => print!("{content}"),
    }

    Ok(())
}

// ─── ファイル収集 ─────────────────────────────────────────────────────────────

fn collect_files(
    dir: &Path,
    include: &str,
    depth: usize,
    verbose: bool,
) -> Result<Vec<ParsedSourceFile>, String> {
    let pattern = format!("{}/{include}", dir.display());
    let paths = glob::glob(&pattern)
        .map_err(|e| format!("invalid glob pattern '{pattern}': {e}"))?;

    let mut files = Vec::new();
    for entry in paths.flatten() {
        // depth チェック: dir からの相対深度
        let rel = entry.strip_prefix(dir).unwrap_or(&entry);
        let file_depth = rel.components().count().saturating_sub(1);
        if file_depth > depth {
            continue;
        }

        let source = std::fs::read_to_string(&entry).unwrap_or_default();
        if source.is_empty() {
            continue;
        }

        let path_str = entry.to_string_lossy().to_string();
        if verbose {
            eprintln!("[arch-gen] scan: reading {path_str}");
        }
        files.push(ParsedSourceFile { path: path_str, source });
    }

    Ok(files)
}

// ─── 出力フォーマット ────────────────────────────────────────────────────────

fn build_scan_text(
    dir: &Path,
    file_count: usize,
    component_names: &[String],
    dependency_pairs: &[(String, String)],
    quality: f64,
    graph: &architecture_reasoner::ArchitectureGraph,
) -> String {
    let mut out = String::new();
    out.push_str("Architecture Scan Result\n");
    out.push_str(&"═".repeat(55));
    out.push('\n');
    out.push_str(&format!("Directory:        {}\n", dir.display()));
    out.push_str(&format!("Files scanned:    {file_count}\n"));
    out.push_str(&format!("Components found: {}\n", component_names.len()));
    out.push_str(&format!("Dependencies:     {}\n\n", dependency_pairs.len()));

    // Layer ごとに分類
    let mut by_layer: std::collections::BTreeMap<String, Vec<&str>> = Default::default();
    for node in &graph.nodes {
        by_layer
            .entry(format!("{:?}", node.layer))
            .or_default()
            .push(&node.name);
    }
    if !by_layer.is_empty() {
        out.push_str("Components by layer:\n");
        for (layer, names) in &by_layer {
            out.push_str(&format!("  [{layer}]\n"));
            for name in names {
                out.push_str(&format!("    - {name}\n"));
            }
        }
        out.push('\n');
    }

    if !dependency_pairs.is_empty() {
        out.push_str("Dependencies:\n");
        for (from, to) in dependency_pairs {
            out.push_str(&format!("  {from} → {to}\n"));
        }
        out.push('\n');
    }

    out.push_str(&format!("Quality Score: {:.4}\n", quality));
    out.push_str(&"═".repeat(55));
    out.push('\n');
    out
}

fn build_scan_json(
    dir: &Path,
    file_count: usize,
    component_names: &[String],
    dependency_pairs: &[(String, String)],
    quality: f64,
) -> Result<String, String> {
    let deps: Vec<serde_json::Value> = dependency_pairs
        .iter()
        .map(|(f, t)| serde_json::json!({"from": f, "to": t}))
        .collect();
    let v = serde_json::json!({
        "directory": dir.display().to_string(),
        "files_scanned": file_count,
        "components": component_names,
        "dependencies": deps,
        "quality_score": quality,
    });
    serde_json::to_string_pretty(&v).map_err(|e| format!("json error: {e}"))
}

fn scan_to_candidate_display(
    component_names: &[String],
    dependency_pairs: &[(String, String)],
    quality: f64,
) -> CandidateDisplay {
    CandidateDisplay {
        score: quality,
        pareto_rank: 0,
        component_names: component_names.to_vec(),
        dependency_pairs: dependency_pairs.to_vec(),
        evaluation: world_model_core::EvaluationVector {
            structural_quality: quality,
            dependency_quality: quality,
            constraint_satisfaction: quality,
            complexity: 1.0 - quality,
            simulation_quality: quality,
        },
        generated_files: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_missing_dir_is_error() {
        let result = run(ScanArgs {
            dir: "/nonexistent/path".to_string(),
            format: "text".to_string(),
            output: None,
            depth: 3,
            include: "**/*.rs".to_string(),
            verbose: false,
        });
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("directory not found"));
    }

    #[test]
    fn test_scan_empty_dir_is_error() {
        let tmp = tempfile::TempDir::new().unwrap();
        let result = run(ScanArgs {
            dir: tmp.path().to_str().unwrap().to_string(),
            format: "text".to_string(),
            output: None,
            depth: 3,
            include: "**/*.rs".to_string(),
            verbose: false,
        });
        assert!(result.is_err());
    }

    #[test]
    fn test_scan_rust_sources_text() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("user_service.rs"),
            "pub struct UserService;\nimpl UserService { pub fn execute() {} }\n",
        )
        .unwrap();
        std::fs::write(
            tmp.path().join("order_service.rs"),
            "use crate::user_service::UserService;\npub fn handle() {}\n",
        )
        .unwrap();

        let result = run(ScanArgs {
            dir: tmp.path().to_str().unwrap().to_string(),
            format: "text".to_string(),
            output: None,
            depth: 3,
            include: "**/*.rs".to_string(),
            verbose: false,
        });
        assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn test_scan_mermaid_output() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("api.rs"), "pub fn handler() {}\n").unwrap();

        let result = run(ScanArgs {
            dir: tmp.path().to_str().unwrap().to_string(),
            format: "mermaid".to_string(),
            output: None,
            depth: 3,
            include: "**/*.rs".to_string(),
            verbose: false,
        });
        assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn test_scan_json_output() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("service.rs"), "pub struct Service;\n").unwrap();

        let result = run(ScanArgs {
            dir: tmp.path().to_str().unwrap().to_string(),
            format: "json".to_string(),
            output: None,
            depth: 3,
            include: "**/*.rs".to_string(),
            verbose: false,
        });
        assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn test_scan_to_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("svc.rs"), "pub struct Svc;\n").unwrap();
        let out_path = tmp.path().join("report.txt");

        let result = run(ScanArgs {
            dir: tmp.path().to_str().unwrap().to_string(),
            format: "text".to_string(),
            output: Some(out_path.to_str().unwrap().to_string()),
            depth: 3,
            include: "**/*.rs".to_string(),
            verbose: false,
        });
        assert!(result.is_ok(), "{:?}", result);
        assert!(out_path.exists());
    }
}
