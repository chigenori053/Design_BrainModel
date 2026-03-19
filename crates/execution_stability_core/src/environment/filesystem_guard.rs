use crate::environment::workspace::Workspace;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct FilesystemGuard;

impl FilesystemGuard {
    pub fn validate_command(
        &self,
        workspace: &Workspace,
        command: &[String],
    ) -> Result<(), String> {
        for arg in command.iter().skip(1) {
            if let Some(path) = candidate_path(arg) {
                self.ensure_within_workspace(workspace, &path)?;
            }
        }
        Ok(())
    }

    pub fn working_dir_hash(&self, workspace: &Workspace) -> Result<String, String> {
        let mut entries = Vec::new();
        collect_entries(
            &workspace.project_root,
            &workspace.project_root,
            &mut entries,
        )?;
        let joined = entries.join("\n");
        Ok(stable_hash_hex(joined.as_bytes()))
    }

    fn ensure_within_workspace(&self, workspace: &Workspace, path: &Path) -> Result<(), String> {
        let candidate = if path.is_absolute() {
            path.to_path_buf()
        } else {
            workspace.project_root.join(path)
        };
        let normalized = normalize_path(&candidate)?;
        let root = normalize_path(&workspace.project_root)?;
        if normalized.starts_with(&root) {
            Ok(())
        } else {
            Err(format!(
                "filesystem guard blocked write outside workspace: {}",
                path.display()
            ))
        }
    }
}

fn candidate_path(arg: &str) -> Option<PathBuf> {
    if arg.starts_with('/') || arg.starts_with("./") || arg.starts_with("../") {
        Some(PathBuf::from(arg))
    } else {
        None
    }
}

fn collect_entries(root: &Path, current: &Path, entries: &mut Vec<String>) -> Result<(), String> {
    for entry in fs::read_dir(current).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let relative = path
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .to_string();
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_dir() {
            entries.push(format!("dir:{relative}"));
            collect_entries(root, &path, entries)?;
        } else {
            let bytes = fs::read(&path).map_err(|error| error.to_string())?;
            entries.push(format!("file:{relative}:{}", stable_hash_hex(&bytes)));
        }
    }
    entries.sort();
    Ok(())
}

fn stable_hash_hex(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn normalize_path(path: &Path) -> Result<PathBuf, String> {
    if path.exists() {
        path.canonicalize().map_err(|error| error.to_string())
    } else if let Some(parent) = path.parent() {
        Ok(parent
            .canonicalize()
            .map_err(|error| error.to_string())?
            .join(path.file_name().unwrap_or_default()))
    } else {
        Ok(path.to_path_buf())
    }
}
