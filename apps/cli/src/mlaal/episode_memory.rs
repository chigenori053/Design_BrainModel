use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};

use super::episode_schema::EpisodeRecord;

#[derive(Debug, Default)]
pub struct EpisodeMemoryStore;

impl EpisodeMemoryStore {
    pub fn load(&self, workspace_root: &Path) -> anyhow::Result<Vec<EpisodeRecord>> {
        let workspace_root = canonical_workspace_root(workspace_root)?;
        let path = self.episodes_path(&workspace_root);
        println!("episodes_path={:?}", path);
        println!("workspace_root={:?}", workspace_root);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read episodes: path={:?}", path))?;
        if raw.trim().is_empty() {
            return Ok(Vec::new());
        }
        let value = serde_json::from_str::<serde_json::Value>(&raw).map_err(|err| {
            anyhow!(
                "failed to parse episodes: path={:?} size={} first_bytes={:?} last_bytes={:?} err={}",
                path,
                raw.len(),
                preview_start(&raw),
                preview_end(&raw),
                err
            )
        })?;
        if !value.is_array() {
            return Err(anyhow!(
                "invalid episodes schema: path={:?} expected=array size={}",
                path,
                raw.len()
            ));
        }
        serde_json::from_value(value).map_err(|err| {
            anyhow!(
                "failed to deserialize episodes: path={:?} size={} err={}",
                path,
                raw.len(),
                err
            )
        })
    }

    pub fn save(&self, workspace_root: &Path, episodes: &[EpisodeRecord]) -> anyhow::Result<()> {
        let workspace_root = canonical_workspace_root(workspace_root)?;
        let path = self.episodes_path(&workspace_root);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        atomic_write(&path, &serde_json::to_vec_pretty(episodes)?)?;
        Ok(())
    }

    pub fn append_episode(
        &self,
        workspace_root: &Path,
        episode: EpisodeRecord,
    ) -> anyhow::Result<()> {
        let mut episodes = self.load(workspace_root)?;
        episodes.push(episode);
        self.save(workspace_root, &episodes)
    }

    pub fn episodes_path(&self, workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(".dbm")
            .join("mlaal")
            .join("episodes.json")
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
