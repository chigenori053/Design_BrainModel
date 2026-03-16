use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct ArchitectureMetadata {
    #[serde(default = "default_ir_version")]
    pub ir_version: String,
    pub version: String,
    pub language: Option<String>,
    pub created_at: u64,
    #[serde(default)]
    pub score: Option<i64>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub evaluation: Option<String>,
}

impl Default for ArchitectureMetadata {
    fn default() -> Self {
        Self {
            ir_version: default_ir_version(),
            version: "0.3".to_string(),
            language: None,
            created_at: 0,
            score: None,
            author: None,
            evaluation: None,
        }
    }
}

fn default_ir_version() -> String {
    "v1".to_string()
}
