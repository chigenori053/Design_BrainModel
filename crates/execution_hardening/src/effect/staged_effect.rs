use super::effect::Effect;
use crate::checksum::{Checksum, ChecksumBuilder};
use crate::error::HardeningError;

/// Manages side-effects under a Copy-on-Write / Diff-based atomic model.
///
/// Execution flow (spec §6.2):
///  1. Effects are staged via `stage()` — they are **not** applied yet.
///  2. On success: `commit()` applies all staged effects atomically.
///     If any single application fails every previously applied effect in
///     this commit is rolled back before the error is returned.
///  3. On failure: `discard_staged()` discards all pending effects without
///     touching physical state.
///
/// Spec §6.4: rollback保証 — `full_rollback()` restores physical state by
/// undoing all *committed* effects in reverse order.
#[derive(Debug, Default)]
pub struct StagedEffectManager {
    /// Effects staged but not yet applied.
    staged: Vec<Effect>,
    /// Effects that have been successfully committed.
    committed: Vec<Effect>,
}

impl StagedEffectManager {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Staging ──────────────────────────────────────────────────────────────

    /// Record an effect as staged.  Does **not** modify physical state.
    pub fn stage(&mut self, effect: Effect) {
        self.staged.push(effect);
    }

    /// How many effects are currently staged.
    pub fn staged_count(&self) -> usize {
        self.staged.len()
    }

    /// How many effects have been committed in this session.
    pub fn committed_count(&self) -> usize {
        self.committed.len()
    }

    // ── Checksum ─────────────────────────────────────────────────────────────

    /// Ordered checksum of staged effects (key + new_value in sequence).
    ///
    /// Satisfies spec §4.3: ordering-dependent, stable serialization.
    pub fn staged_checksum(&self) -> Checksum {
        let mut builder = ChecksumBuilder::new();
        for e in &self.staged {
            builder = builder
                .update_str(&e.stable_key())
                .update(e.new_value_bytes());
        }
        builder.finish()
    }

    /// Ordered checksum of committed effects.
    pub fn committed_checksum(&self) -> Checksum {
        let mut builder = ChecksumBuilder::new();
        for e in &self.committed {
            builder = builder
                .update_str(&e.stable_key())
                .update(e.new_value_bytes());
        }
        builder.finish()
    }

    // ── Commit ───────────────────────────────────────────────────────────────

    /// Apply all staged effects atomically.
    ///
    /// On any application failure the effects applied so far in **this call**
    /// are rolled back and `Err(EffectApplyFailed)` is returned.
    /// The `staged` list is cleared regardless of outcome.
    ///
    /// Spec §6: 成功時のみ commit
    pub fn commit(&mut self) -> Result<(), HardeningError> {
        let to_commit: Vec<Effect> = std::mem::take(&mut self.staged);
        let mut applied: Vec<Effect> = Vec::with_capacity(to_commit.len());

        for effect in &to_commit {
            match Self::apply_effect(effect) {
                Ok(()) => applied.push(effect.clone()),
                Err(msg) => {
                    // Rollback everything applied in this commit attempt.
                    for prev in applied.iter().rev() {
                        let _ = Self::rollback_effect(prev);
                    }
                    return Err(HardeningError::EffectApplyFailed(format!(
                        "Effect '{key}' failed: {msg}; commit rolled back",
                        key = effect.stable_key()
                    )));
                }
            }
        }

        self.committed.extend(to_commit);
        Ok(())
    }

    // ── Discard / Rollback ────────────────────────────────────────────────────

    /// Discard all staged effects without applying them.
    ///
    /// Physical state is **untouched**.  Spec §6: 失敗時は完全破棄.
    pub fn discard_staged(&mut self) {
        self.staged.clear();
    }

    /// Rollback all *committed* effects in reverse order.
    ///
    /// This is the full rollback guarantee — `state_corruption == 0`.
    /// Spec §6.4: 物理状態を完全復元 / 部分変更ゼロ
    pub fn full_rollback(&mut self) -> Result<(), HardeningError> {
        let committed = std::mem::take(&mut self.committed);
        for effect in committed.iter().rev() {
            Self::rollback_effect(effect).map_err(|msg| {
                HardeningError::RollbackFailed(format!(
                    "Could not rollback '{}': {msg}",
                    effect.stable_key()
                ))
            })?;
        }
        Ok(())
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn apply_effect(effect: &Effect) -> Result<(), String> {
        match effect {
            Effect::FileWrite { path, content, .. } => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                std::fs::write(path, content).map_err(|e| e.to_string())
            }
            Effect::FileDelete { path, .. } => {
                std::fs::remove_file(path).map_err(|e| e.to_string())
            }
            // In-memory effects are handled by the caller's state layer;
            // the manager only tracks them for rollback metadata.
            Effect::StateSet { .. } | Effect::MemoryUpdate { .. } => Ok(()),
        }
    }

    fn rollback_effect(effect: &Effect) -> Result<(), String> {
        match effect {
            Effect::FileWrite {
                path,
                previous_content,
                ..
            } => match previous_content {
                Some(prev) => std::fs::write(path, prev).map_err(|e| e.to_string()),
                None => std::fs::remove_file(path).map_err(|e| e.to_string()),
            },
            Effect::FileDelete {
                path,
                previous_content,
            } => {
                if let Some(parent) = path.parent() {
                    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
                }
                std::fs::write(path, previous_content).map_err(|e| e.to_string())
            }
            Effect::StateSet { .. } | Effect::MemoryUpdate { .. } => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn file_write_effect(path: PathBuf, content: &[u8]) -> Effect {
        Effect::FileWrite {
            path,
            content: content.to_vec(),
            previous_content: None,
        }
    }

    #[test]
    fn stage_does_not_apply() {
        let dir = std::env::temp_dir();
        let path = dir.join("staged_no_apply_test.txt");
        let _ = std::fs::remove_file(&path);

        let mut mgr = StagedEffectManager::new();
        mgr.stage(file_write_effect(path.clone(), b"data"));

        assert!(!path.exists(), "staged effect must not touch disk");
    }

    #[test]
    fn commit_applies_effects() {
        let file = NamedTempFile::new().unwrap();
        let path: PathBuf = file.path().to_path_buf();
        drop(file); // close so we can rewrite

        let mut mgr = StagedEffectManager::new();
        mgr.stage(file_write_effect(path.clone(), b"committed"));
        mgr.commit().unwrap();

        let content = std::fs::read(&path).unwrap();
        assert_eq!(content, b"committed");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn discard_staged_leaves_state_unchanged() {
        let path = std::env::temp_dir().join("discard_test_c5.txt");
        let _ = std::fs::remove_file(&path);

        let mut mgr = StagedEffectManager::new();
        mgr.stage(file_write_effect(path.clone(), b"should-not-exist"));
        mgr.discard_staged();
        mgr.commit().unwrap(); // nothing staged — no-op

        assert!(!path.exists());
    }

    #[test]
    fn full_rollback_restores_file() {
        let dir = std::env::temp_dir();
        let path = dir.join("rollback_c5_test.txt");
        std::fs::write(&path, b"original").unwrap();

        let mut mgr = StagedEffectManager::new();
        mgr.stage(Effect::FileWrite {
            path: path.clone(),
            content: b"overwritten".to_vec(),
            previous_content: Some(b"original".to_vec()),
        });
        mgr.commit().unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"overwritten");

        mgr.full_rollback().unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"original");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn checksum_is_deterministic() {
        let mut m1 = StagedEffectManager::new();
        m1.stage(Effect::StateSet {
            key: "k".into(),
            value: b"v".to_vec(),
            previous_value: None,
        });

        let mut m2 = StagedEffectManager::new();
        m2.stage(Effect::StateSet {
            key: "k".into(),
            value: b"v".to_vec(),
            previous_value: None,
        });

        assert_eq!(m1.staged_checksum(), m2.staged_checksum());
    }
}
