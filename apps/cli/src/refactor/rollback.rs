use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    pub entries: Vec<WorkspaceEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceEntry {
    pub path: PathBuf,
    pub original: Option<Vec<u8>>,
}

pub fn snapshot_workspace(root: &Path, files: &[PathBuf]) -> Result<WorkspaceSnapshot, String> {
    let mut entries = Vec::new();
    for file in files {
        let path = if file.is_absolute() {
            file.clone()
        } else {
            root.join(file)
        };
        let original = if path.exists() {
            Some(
                fs::read(&path)
                    .map_err(|err| format!("failed to snapshot {}: {err}", path.display()))?,
            )
        } else {
            None
        };
        entries.push(WorkspaceEntry { path, original });
    }
    Ok(WorkspaceSnapshot { entries })
}

pub fn rollback_apply(snapshot: &WorkspaceSnapshot) -> Result<(), String> {
    for entry in &snapshot.entries {
        match &entry.original {
            Some(bytes) => fs::write(&entry.path, bytes)
                .map_err(|err| format!("failed to restore {}: {err}", entry.path.display()))?,
            None => {
                if entry.path.exists() {
                    fs::remove_file(&entry.path).map_err(|err| {
                        format!("failed to remove {}: {err}", entry.path.display())
                    })?;
                }
            }
        }
    }
    Ok(())
}
