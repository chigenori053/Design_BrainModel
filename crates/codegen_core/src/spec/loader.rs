use std::path::Path;

use super::LanguageSpec;

#[derive(Debug)]
pub enum LoadError {
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "IO error: {e}"),
            LoadError::Json(e) => write!(f, "JSON parse error: {e}"),
        }
    }
}

impl std::error::Error for LoadError {}

impl From<std::io::Error> for LoadError {
    fn from(e: std::io::Error) -> Self {
        LoadError::Io(e)
    }
}

impl From<serde_json::Error> for LoadError {
    fn from(e: serde_json::Error) -> Self {
        LoadError::Json(e)
    }
}

/// Deserialize a LanguageSpec from a JSON string.
pub fn load_from_str(json: &str) -> Result<LanguageSpec, LoadError> {
    Ok(serde_json::from_str(json)?)
}

/// Read a JSON file from disk and deserialize it into a LanguageSpec.
pub fn load_from_file(path: &Path) -> Result<LanguageSpec, LoadError> {
    let json = std::fs::read_to_string(path)?;
    load_from_str(&json)
}
