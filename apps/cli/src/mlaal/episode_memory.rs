use std::fs;
use std::path::{Path, PathBuf};

use super::episode_schema::EpisodeRecord;

#[derive(Debug, Default)]
pub struct EpisodeMemoryStore;

impl EpisodeMemoryStore {
    pub fn load(&self, workspace_root: &Path) -> anyhow::Result<Vec<EpisodeRecord>> {
        let path = self.episodes_path(workspace_root);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn save(&self, workspace_root: &Path, episodes: &[EpisodeRecord]) -> anyhow::Result<()> {
        let path = self.episodes_path(workspace_root);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, serde_json::to_vec_pretty(episodes)?)?;
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
