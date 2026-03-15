use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct ArchitectureMetadata {
    pub version: String,
    pub language: Option<String>,
    pub created_at: u64,
}

impl Default for ArchitectureMetadata {
    fn default() -> Self {
        Self {
            version: "0.3".to_string(),
            language: None,
            created_at: 0,
        }
    }
}
