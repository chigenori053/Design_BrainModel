use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};
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
        let workspace_root = canonical_workspace_root(workspace_root)?;
        let path = Self::policy_path(&workspace_root);
        println!("policy_path={:?}", path);
        println!("workspace_root={:?}", workspace_root);
        if !path.exists() {
            return Ok(Self::default());
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read adaptive policy: path={:?}", path))?;
        if raw.trim().is_empty() {
            return Ok(Self::default());
        }
        let value = serde_json::from_str::<serde_json::Value>(&raw).map_err(|err| {
            anyhow!(
                "failed to parse adaptive policy: path={:?} size={} first_bytes={:?} last_bytes={:?} err={}",
                path,
                raw.len(),
                preview_start(&raw),
                preview_end(&raw),
                err
            )
        })?;
        if !value.is_object() {
            return Err(anyhow!(
                "invalid adaptive policy schema: path={:?} expected=object size={}",
                path,
                raw.len()
            ));
        }
        serde_json::from_value(value).map_err(|err| {
            anyhow!(
                "failed to deserialize adaptive policy: path={:?} size={} err={}",
                path,
                raw.len(),
                err
            )
        })
    }

    pub fn save(&self, workspace_root: &Path) -> anyhow::Result<()> {
        let workspace_root = canonical_workspace_root(workspace_root)?;
        let path = Self::policy_path(&workspace_root);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        atomic_write(&path, &serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn policy_path(workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(".dbm")
            .join("mlaal")
            .join("policy.json")
    }
}

fn canonical_workspace_root(workspace_root: &Path) -> anyhow::Result<PathBuf> {
    let root = workspace_root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize workspace_root={:?}", workspace_root))?;
    Ok(find_workspace_boundary(&root).unwrap_or(root))
}

fn find_workspace_boundary(root: &Path) -> Option<PathBuf> {
    for candidate in root.ancestors() {
        let cargo_toml = candidate.join("Cargo.toml");
        if cargo_toml.exists()
            && fs::read_to_string(&cargo_toml)
                .map(|raw| raw.contains("[workspace]"))
                .unwrap_or(false)
        {
            return Some(candidate.to_path_buf());
        }
    }
    root.ancestors()
        .find(|candidate| candidate.join(".git").exists())
        .map(Path::to_path_buf)
}

fn preview_start(raw: &str) -> String {
    raw.chars().take(120).collect()
}

fn preview_end(raw: &str) -> String {
    raw.chars()
        .rev()
        .take(120)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

fn atomic_write(path: &Path, data: &[u8]) -> anyhow::Result<()> {
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, data)?;
    fs::rename(&tmp, path)?;
    Ok(())
}
