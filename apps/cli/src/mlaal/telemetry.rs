use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, anyhow};

use super::telemetry_schema::{TelemetryRecord, TelemetryWindowKpi};

#[derive(Debug, Default)]
pub struct TelemetryStore;

impl TelemetryStore {
    pub fn load(&self, workspace_root: &Path) -> anyhow::Result<Vec<TelemetryRecord>> {
        let workspace_root = canonical_workspace_root(workspace_root)?;
        let path = self.telemetry_path(&workspace_root);
        println!("telemetry_path={:?}", path);
        println!("workspace_root={:?}", workspace_root);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read telemetry: path={:?}", path))?;
        if raw.trim().is_empty() {
            return Ok(Vec::new());
        }
        let value = serde_json::from_str::<serde_json::Value>(&raw).map_err(|err| {
            anyhow!(
                "failed to parse telemetry: path={:?} size={} first_bytes={:?} last_bytes={:?} err={}",
                path,
                raw.len(),
                preview_start(&raw),
                preview_end(&raw),
                err
            )
        })?;
        if !value.is_array() {
            return Err(anyhow!(
                "invalid telemetry schema: path={:?} expected=array size={}",
                path,
                raw.len()
            ));
        }
        serde_json::from_value(value).map_err(|err| {
            anyhow!(
                "failed to deserialize telemetry: path={:?} size={} err={}",
                path,
                raw.len(),
                err
            )
        })
    }

    pub fn save(&self, workspace_root: &Path, records: &[TelemetryRecord]) -> anyhow::Result<()> {
        let workspace_root = canonical_workspace_root(workspace_root)?;
        let path = self.telemetry_path(&workspace_root);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        atomic_write(&path, &serde_json::to_vec_pretty(records)?)?;
        Ok(())
    }

    pub fn append(&self, workspace_root: &Path, record: TelemetryRecord) -> anyhow::Result<()> {
        let mut records = self.load(workspace_root)?;
        records.push(record);
        self.save(workspace_root, &records)
    }

    pub fn latest_window(
        &self,
        workspace_root: &Path,
        window: usize,
    ) -> anyhow::Result<Vec<TelemetryRecord>> {
        let mut records = self.load(workspace_root)?;
        if records.len() > window {
            records = records.split_off(records.len() - window);
        }
        Ok(records)
    }

    pub fn compute_kpi(&self, records: &[TelemetryRecord]) -> TelemetryWindowKpi {
        if records.is_empty() {
            return TelemetryWindowKpi::default();
        }

        let count = records.len() as f32;
        let recall_hits = records.iter().filter(|record| record.recall_hit).count() as f32;
        let skipped = records
            .iter()
            .filter(|record| record.rollout_skipped)
            .count() as f32;
        let rollback_free_recall = records
            .iter()
            .filter(|record| record.recall_hit && record.rollback_free)
            .count() as f32;
        let replay_drift = records
            .iter()
            .filter(|record| record.replay_divergence > 0.55)
            .count() as f32;

        TelemetryWindowKpi {
            avg_rollout_depth: records
                .iter()
                .map(|record| record.rollout_depth as f32)
                .sum::<f32>()
                / count,
            recall_hit_rate: recall_hits / count,
            rollout_skip_rate: skipped / count,
            safe_reuse_success_rate: if recall_hits > 0.0 {
                rollback_free_recall / recall_hits
            } else {
                0.0
            },
            avg_preview_latency_ms: records
                .iter()
                .map(|record| record.preview_latency_ms as f32)
                .sum::<f32>()
                / count,
            replay_drift_rate: replay_drift / count,
        }
    }

    pub fn telemetry_path(&self, workspace_root: &Path) -> PathBuf {
        workspace_root
            .join(".dbm")
            .join("mlaal")
            .join("telemetry.json")
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
