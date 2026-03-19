use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default)]
pub struct LockManager;

impl LockManager {
    pub fn ensure_lockfile(&self, project_root: &Path) -> Result<PathBuf, String> {
        let path = project_root.join("execution.lock");
        if path.exists() {
            Ok(path)
        } else {
            Err(format!("lockfile missing: {}", path.display()))
        }
    }

    pub fn lockfile_hash(&self, project_root: &Path) -> Result<String, String> {
        let path = self.ensure_lockfile(project_root)?;
        let contents = fs::read(&path).map_err(|error| error.to_string())?;
        Ok(stable_hash_hex(&contents))
    }
}

fn stable_hash_hex(bytes: &[u8]) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
