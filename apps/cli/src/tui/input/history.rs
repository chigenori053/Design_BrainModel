use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentInputHistory {
    pub path: PathBuf,
}

impl PersistentInputHistory {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn load(&self) -> Vec<String> {
        fs::read_to_string(&self.path)
            .map(|content| {
                content
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn append(&self, input: &str) -> Result<(), String> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| format!("history path has no parent: {}", self.path.display()))?;
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create history dir {}: {err}", parent.display()))?;
        let mut existing = self.load();
        existing.push(input.to_string());
        fs::write(&self.path, existing.join("\n") + "\n")
            .map_err(|err| format!("failed to write history {}: {err}", self.path.display()))
    }
}
