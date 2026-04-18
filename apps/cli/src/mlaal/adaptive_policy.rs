use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AdaptivePolicy {
    pub resonance_threshold: f32,
    pub skip_threshold: f32,
    pub decay_lambda: f32,
    pub beam_shrink_ratio: f32,
    pub depth_shrink_ratio: f32,
}

impl Default for AdaptivePolicy {
    fn default() -> Self {
        Self {
            resonance_threshold: 0.92,
            skip_threshold: 0.92,
            decay_lambda: 0.08,
            beam_shrink_ratio: 0.50,
            depth_shrink_ratio: 0.34,
        }
    }
}

impl AdaptivePolicy {
    pub fn load(workspace_root: &Path) -> anyhow::Result<Self> {
        let path = Self::policy_path(workspace_root);
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn save(&self, workspace_root: &Path) -> anyhow::Result<()> {
        let path = Self::policy_path(workspace_root);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn policy_path(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(".dbm")
            .join("mlaal")
            .join("policy.json")
    }
}
