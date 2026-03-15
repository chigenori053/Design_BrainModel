use std::fs;
use std::path::Path;

use crate::input_bridge::load_design_file;
use crate::output::markdown::build_markdown;
use crate::output::mermaid::build_mermaid;
use crate::output::text::{CandidateDisplay, render_evaluation};

/// `export` コマンド: 保存済み design.json を指定フォーマットで出力する。
///
/// フォーマット:
///   - `json`      : design.json をそのまま整形出力
///   - `mermaid`   : 各候補の Mermaid graph TD
///   - `markdown`  : Markdown レポート
///   - `text`      : テキストサマリー
pub fn run(design_file: &str, format: &str, output: Option<&str>) -> Result<(), String> {
    let path = Path::new(design_file);
    let design = load_design_file(path)?;

    let content = match format {
        "json" => serde_json::to_string_pretty(&design)
            .map_err(|e| format!("json serialization failed: {e}"))?,

        "mermaid" => {
            let mut out = String::new();
            for c in &design.candidates {
                out.push_str(&format!(
                    "--- Candidate {} (score: {:.4}) ---\n",
                    c.id, c.score
                ));
                out.push_str(&build_mermaid(&c.components, &to_dep_pairs(&c.dependencies)));
                out.push('\n');
            }
            out
        }

        "markdown" => {
            let displays = to_candidate_displays(&design.candidates);
            build_markdown(&design.input, design.search_states, &displays)
        }

        "text" | _ => {
            let mut out = String::new();
            out.push_str("Architecture Export\n");
            out.push_str(&"═".repeat(55));
            out.push('\n');
            out.push_str(&format!("Input: \"{}\"\n", design.input));
            out.push_str(&format!("Search states: {}\n\n", design.search_states));
            for c in &design.candidates {
                out.push_str(&format!(
                    "─── Candidate {} (Score: {:.4}) {}\n",
                    c.id,
                    c.score,
                    "─".repeat(28)
                ));
                let eval = world_model_core::EvaluationVector {
                    structural_quality: c.evaluation.structural_quality,
                    dependency_quality: c.evaluation.dependency_quality,
                    constraint_satisfaction: c.evaluation.constraint_satisfaction,
                    complexity: c.evaluation.complexity,
                    simulation_quality: c.evaluation.simulation_quality,
                };
                out.push_str(&render_evaluation(&eval));
                out.push('\n');
            }
            out.push_str(&"═".repeat(55));
            out.push('\n');
            out
        }
    };

    match output {
        Some(out_path) => {
            let out = Path::new(out_path);
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("failed to create output dir: {e}"))?;
            }
            fs::write(out, &content)
                .map_err(|e| format!("failed to write to '{}': {e}", out.display()))?;
            eprintln!("[arch-gen] exported to {}", out.display());
        }
        None => print!("{content}"),
    }

    Ok(())
}

// ─── ヘルパー ────────────────────────────────────────────────────────────────

fn to_dep_pairs(deps: &[[String; 2]]) -> Vec<(String, String)> {
    deps.iter().map(|d| (d[0].clone(), d[1].clone())).collect()
}

fn to_candidate_displays(
    candidates: &[crate::input_bridge::SavedCandidate],
) -> Vec<CandidateDisplay> {
    candidates
        .iter()
        .map(|c| CandidateDisplay {
            score: c.score,
            pareto_rank: c.pareto_rank,
            component_names: c.components.clone(),
            dependency_pairs: to_dep_pairs(&c.dependencies),
            evaluation: world_model_core::EvaluationVector {
                structural_quality: c.evaluation.structural_quality,
                dependency_quality: c.evaluation.dependency_quality,
                constraint_satisfaction: c.evaluation.constraint_satisfaction,
                complexity: c.evaluation.complexity,
                simulation_quality: c.evaluation.simulation_quality,
            },
            generated_files: vec![],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input_bridge::{SavedCandidate, SavedCodeMetrics, SavedDesign, SavedEvaluation, save_design_file};

    fn make_design() -> SavedDesign {
        SavedDesign {
            version: "1.0".to_string(),
            generated_at: "1742000000".to_string(),
            input: "ECサイト".to_string(),
            search_states: 30,
            candidates: vec![SavedCandidate {
                id: 1,
                score: 0.80,
                pareto_rank: 0,
                evaluation: SavedEvaluation {
                    structural_quality: 0.9,
                    dependency_quality: 0.8,
                    constraint_satisfaction: 0.9,
                    complexity: 0.3,
                    simulation_quality: 0.9,
                    total: 0.76,
                },
                components: vec!["service_1".to_string(), "database_2".to_string()],
                dependencies: vec![["service_1".to_string(), "database_2".to_string()]],
                code_metrics: SavedCodeMetrics::default(),
            }],
        }
    }

    fn write_tmp_design() -> (tempfile::NamedTempFile, SavedDesign) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let design = make_design();
        save_design_file(&design, tmp.path()).unwrap();
        (tmp, design)
    }

    #[test]
    fn test_export_missing_file_is_error() {
        let result = run("/nonexistent/design.json", "text", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_export_json_format() {
        let (tmp, _) = write_tmp_design();
        let result = run(tmp.path().to_str().unwrap(), "json", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_export_mermaid_format() {
        let (tmp, _) = write_tmp_design();
        let result = run(tmp.path().to_str().unwrap(), "mermaid", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_export_markdown_format() {
        let (tmp, _) = write_tmp_design();
        let result = run(tmp.path().to_str().unwrap(), "markdown", None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_export_to_file() {
        let (tmp, _) = write_tmp_design();
        let out_dir = tempfile::TempDir::new().unwrap();
        let out_path = out_dir.path().join("output.md");
        let result = run(
            tmp.path().to_str().unwrap(),
            "markdown",
            Some(out_path.to_str().unwrap()),
        );
        assert!(result.is_ok());
        assert!(out_path.exists());
    }
}
