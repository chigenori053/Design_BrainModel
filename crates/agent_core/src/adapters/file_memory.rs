use std::fs;
use std::path::{Path, PathBuf};

use crate::domain::DomainError;
use crate::ports::MemoryPort;

#[derive(Clone, Debug)]
pub struct FileMemory {
    root: PathBuf,
}

impl FileMemory {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    fn key_path(&self, key: &str) -> PathBuf {
        self.root.join(format!("{key}.bin"))
    }
}

impl MemoryPort for FileMemory {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, DomainError> {
        let path = self.key_path(key);
        if !path.exists() {
            return Ok(None);
        }
        fs::read(path)
            .map(Some)
            .map_err(|e| DomainError::PortError(format!("memory read failed: {e}")))
    }

    fn put(&self, key: &str, value: &[u8]) -> Result<(), DomainError> {
        fs::create_dir_all(&self.root)
            .map_err(|e| DomainError::PortError(format!("memory mkdir failed: {e}")))?;
        fs::write(self.key_path(key), value)
            .map_err(|e| DomainError::PortError(format!("memory write failed: {e}")))
    }
}
