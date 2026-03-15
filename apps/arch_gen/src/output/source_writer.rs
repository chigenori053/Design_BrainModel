use std::fs;
use std::path::{Path, PathBuf};

pub struct GeneratedFile {
    pub path: String,
    pub contents: String,
}

pub struct SourceTree {
    pub files: Vec<GeneratedFile>,
}

/// `SourceTree` を `<output_dir>/candidate_<id>/` に書き出す。
/// 書き出したファイルのパス一覧を返す。
pub fn write_source_tree(
    source_tree: &SourceTree,
    output_dir: &Path,
    candidate_id: usize,
) -> Result<Vec<PathBuf>, String> {
    let dir = output_dir.join(format!("candidate_{candidate_id}"));
    fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create output dir '{}': {e}", dir.display()))?;

    let mut written = Vec::new();
    for file in &source_tree.files {
        let dest = dir.join(&file.path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("failed to create dir '{}': {e}", parent.display()))?;
        }
        fs::write(&dest, &file.contents)
            .map_err(|e| format!("failed to write '{}': {e}", dest.display()))?;
        written.push(dest);
    }

    Ok(written)
}
