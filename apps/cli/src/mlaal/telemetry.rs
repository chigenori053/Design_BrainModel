use std::fs;
use std::path::{Path, PathBuf};

use super::telemetry_schema::{TelemetryRecord, TelemetryWindowKpi};

#[derive(Debug, Default)]
pub struct TelemetryStore;

impl TelemetryStore {
    pub fn load(&self, workspace_root: &Path) -> anyhow::Result<Vec<TelemetryRecord>> {
        let path = self.telemetry_path(workspace_root);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let raw = fs::read_to_string(&path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    pub fn save(&self, workspace_root: &Path, records: &[TelemetryRecord]) -> anyhow::Result<()> {
        let path = self.telemetry_path(workspace_root);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, serde_json::to_vec_pretty(records)?)?;
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
