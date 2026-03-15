use std::fs;
use std::path::{Path, PathBuf};

use code_ir::SourceTree;

/// 書き出し戦略
#[derive(Debug, Clone, PartialEq, Default)]
pub enum OutputStrategy {
    /// 新規ディレクトリに作成（default）
    #[default]
    New,
    /// 既存ディレクトリにマージ（衝突はスキップ）
    Merge,
    /// 既存ファイルを上書き
    Overwrite,
    /// 書き出しをせず対象ファイル一覧のみ表示
    DryRun,
}

impl OutputStrategy {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "new" => Ok(Self::New),
            "merge" => Ok(Self::Merge),
            "overwrite" => Ok(Self::Overwrite),
            "dry-run" => Ok(Self::DryRun),
            other => Err(format!(
                "unknown output-strategy '{other}'; expected: new | merge | overwrite | dry-run"
            )),
        }
    }
}

/// 出力レイアウト
#[derive(Debug, Clone, PartialEq, Default)]
pub enum OutputLayout {
    /// src/ 直下にすべて配置（default）
    #[default]
    Flat,
    /// モジュール階層を反映したディレクトリ構造
    Module,
}

impl OutputLayout {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "flat" => Ok(Self::Flat),
            "module" => Ok(Self::Module),
            other => Err(format!(
                "unknown output-layout '{other}'; expected: flat | module"
            )),
        }
    }
}

/// `code_ir::SourceTree` を strategy / layout に応じて書き出す。
pub fn write_source_tree_with_options(
    source_tree: &SourceTree,
    output_dir: &Path,
    candidate_id: usize,
    strategy: &OutputStrategy,
    layout: &OutputLayout,
) -> Result<Vec<PathBuf>, String> {
    let base = output_dir.join(format!("candidate_{candidate_id}"));

    // dry-run は書き出しなしでファイル一覧を表示して終了
    if *strategy == OutputStrategy::DryRun {
        println!(
            "[dry-run] Would write {} file(s) to {}:",
            source_tree.files.len(),
            base.display()
        );
        for file in &source_tree.files {
            let dest = resolve_dest(&base, &file.path, layout);
            let status = if dest.exists() { "SKIP" } else { "NEW " };
            println!("  {status}  {}", dest.display());
        }
        return Ok(vec![]);
    }

    fs::create_dir_all(&base)
        .map_err(|e| format!("failed to create output dir '{}': {e}", base.display()))?;

    let mut written = Vec::new();
    for file in &source_tree.files {
        let dest = resolve_dest(&base, &file.path, layout);

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create dir '{}': {e}", parent.display()))?;
        }

        match strategy {
            OutputStrategy::Merge if dest.exists() => {
                eprintln!("[arch_gen] SKIP (exists): {}", dest.display());
                continue;
            }
            OutputStrategy::New | OutputStrategy::Merge | OutputStrategy::Overwrite => {
                fs::write(&dest, &file.content)
                    .map_err(|e| format!("failed to write '{}': {e}", dest.display()))?;
                written.push(dest);
            }
            OutputStrategy::DryRun => unreachable!(),
        }
    }

    Ok(written)
}

fn resolve_dest(base: &Path, file_path: &str, layout: &OutputLayout) -> PathBuf {
    match layout {
        OutputLayout::Flat => {
            // ファイル名のみを使用（ディレクトリ構造を平坦化）
            let name = Path::new(file_path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| file_path.replace('/', "_"));
            base.join(name)
        }
        OutputLayout::Module => {
            // ファイルパスそのままを使用
            base.join(file_path)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_ir::{SourceFile, SourceTree};

    fn make_tree() -> SourceTree {
        SourceTree {
            files: vec![
                SourceFile {
                    path: "service_1.rs".to_string(),
                    content: "pub fn run() {}".to_string(),
                },
                SourceFile {
                    path: "database_2.rs".to_string(),
                    content: "pub struct Db;".to_string(),
                },
            ],
        }
    }

    #[test]
    fn test_write_new_strategy() {
        let tmp = tempfile::TempDir::new().unwrap();
        let written = write_source_tree_with_options(
            &make_tree(),
            tmp.path(),
            1,
            &OutputStrategy::New,
            &OutputLayout::Flat,
        )
        .unwrap();
        assert_eq!(written.len(), 2);
        assert!(written.iter().all(|p| p.exists()));
    }

    #[test]
    fn test_dry_run_writes_nothing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let written = write_source_tree_with_options(
            &make_tree(),
            tmp.path(),
            1,
            &OutputStrategy::DryRun,
            &OutputLayout::Flat,
        )
        .unwrap();
        assert!(written.is_empty());
        assert!(!tmp.path().join("candidate_1").exists());
    }

    #[test]
    fn test_merge_skips_existing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path().join("candidate_1");
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("service_1.rs"), "existing").unwrap();

        let written = write_source_tree_with_options(
            &make_tree(),
            tmp.path(),
            1,
            &OutputStrategy::Merge,
            &OutputLayout::Flat,
        )
        .unwrap();
        // service_1.rs はスキップ、database_2.rs のみ書き出し
        assert_eq!(written.len(), 1);
        assert_eq!(
            fs::read_to_string(base.join("service_1.rs")).unwrap(),
            "existing"
        );
    }

    #[test]
    fn test_overwrite_replaces_existing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let base = tmp.path().join("candidate_1");
        fs::create_dir_all(&base).unwrap();
        fs::write(base.join("service_1.rs"), "old content").unwrap();

        write_source_tree_with_options(
            &make_tree(),
            tmp.path(),
            1,
            &OutputStrategy::Overwrite,
            &OutputLayout::Flat,
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(base.join("service_1.rs")).unwrap(),
            "pub fn run() {}"
        );
    }

    #[test]
    fn test_module_layout_preserves_path() {
        let tmp = tempfile::TempDir::new().unwrap();
        let tree = SourceTree {
            files: vec![SourceFile {
                path: "services/user.rs".to_string(),
                content: "pub struct User;".to_string(),
            }],
        };
        let written = write_source_tree_with_options(
            &tree,
            tmp.path(),
            1,
            &OutputStrategy::New,
            &OutputLayout::Module,
        )
        .unwrap();
        assert!(written[0].ends_with("services/user.rs"));
    }

    #[test]
    fn test_strategy_from_str() {
        assert_eq!(
            OutputStrategy::from_str("new").unwrap(),
            OutputStrategy::New
        );
        assert_eq!(
            OutputStrategy::from_str("merge").unwrap(),
            OutputStrategy::Merge
        );
        assert_eq!(
            OutputStrategy::from_str("overwrite").unwrap(),
            OutputStrategy::Overwrite
        );
        assert_eq!(
            OutputStrategy::from_str("dry-run").unwrap(),
            OutputStrategy::DryRun
        );
        assert!(OutputStrategy::from_str("invalid").is_err());
    }
}
