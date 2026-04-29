use std::path::PathBuf;

use crate::design_delta::{
    CodingPatchPlan, DesignDelta, MutationCandidate, MutationPlan, MutationSearchResult,
    RationalityScore, TradeoffExplanation,
};
use crate::service::dto::{ActionKind, IRActiveTransaction, IRState, SessionAppliedDiff};
use uuid::Uuid;

use super::types::ExecutionPlan;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConversationState {
    pub autonomous_label: Option<String>,
    pub last_target: Option<PathBuf>,
    pub last_node: Option<String>,
    pub last_plan: Option<ExecutionPlan>,
    pub last_accepted_plan_id: Option<Uuid>,
    pub last_viewer_session: Option<String>,
    pub last_analysis_summary: Option<String>,
    pub ir_state: IRState,
    /// Phase B-2 (DBM-IR-STATE-SPEC v1.0): per-file IR snapshot manager.
    ///
    /// Tracks `IrState` (snapshot + dirty flag) for each file the session has
    /// analysed or refactored.  Single source of truth for IR state.
    pub ir_state_manager: crate::ir_state::IrStateManager,
    pub hook_promotion_count: u64,
    pub hook_false_promotion_count: u64,
    pub last_design_delta: Option<DesignDelta>,
    pub last_patch_plan: Option<CodingPatchPlan>,
    pub active_mutation_plan: Option<MutationPlan>,
    pub last_rationality_score: Option<RationalityScore>,
    pub mutation_candidates: Vec<MutationCandidate>,
    pub selected_mutation: Option<MutationCandidate>,
    pub mutation_search_depth: usize,
    pub last_mutation_search_result: Option<MutationSearchResult>,
    pub last_tradeoff_explanation: Option<TradeoffExplanation>,
}

impl ConversationState {
    pub fn prompt_label(&self) -> Option<&str> {
        self.autonomous_label.as_deref().or_else(|| {
            self.last_node.as_deref().or_else(|| {
                self.last_target
                    .as_ref()
                    .and_then(|path| path.file_name())
                    .and_then(|name| name.to_str())
            })
        })
    }

    pub fn active_transaction(&self) -> Option<&IRActiveTransaction> {
        self.ir_state.active_transaction.as_ref()
    }

    pub fn active_transaction_mut(&mut self) -> Option<&mut IRActiveTransaction> {
        self.ir_state.active_transaction.as_mut()
    }

    pub fn set_active_transaction(&mut self, tx: IRActiveTransaction) {
        self.ir_state.active_transaction = Some(tx);
    }

    pub fn clear_active_transaction(&mut self) {
        self.ir_state.active_transaction = None;
        self.ir_state.next_allowed_actions.clear();
    }

    pub fn has_pending_transaction(&self) -> bool {
        self.active_transaction()
            .map(|tx| tx.pending && !tx.applied)
            .unwrap_or(false)
    }

    pub fn note_target(&mut self, target: PathBuf) {
        self.last_target = Some(target.clone());
        self.ir_state.current_target = Some(target);
    }

    pub fn start_preview_transaction(&mut self, target: PathBuf) {
        let transaction_id = format!("tx:{}", target.display());
        self.note_target(target.clone());
        self.ir_state.active_transaction = Some(IRActiveTransaction {
            transaction_id,
            canonical_target: target,
            pending: true,
            applied: false,
            validated: false,
            rollback_available: false,
            latest_diff_ref: None,
            latest_build_ok: None,
            file_hash: None,
        });
        self.ir_state.next_allowed_actions =
            vec![ActionKind::Apply, ActionKind::Refactor, ActionKind::Analyze];
    }

    pub fn mark_transaction_applied(&mut self, snapshot: Option<SessionAppliedDiff>) {
        if let Some(tx) = self.ir_state.active_transaction.as_mut() {
            tx.pending = false;
            tx.applied = true;
            tx.validated = false;
            tx.rollback_available = true;
            tx.latest_diff_ref = snapshot;
        }
        self.ir_state.next_allowed_actions = vec![
            ActionKind::Validate,
            ActionKind::Refactor,
            ActionKind::Rollback,
        ];
    }

    pub fn apply_transaction(&mut self) -> Result<Option<SessionAppliedDiff>, String> {
        let snapshot = self
            .ir_state
            .active_transaction
            .as_ref()
            .ok_or_else(|| "no active transaction".to_string())?
            .latest_diff_ref
            .clone();
        self.mark_transaction_applied(snapshot.clone());
        Ok(snapshot)
    }

    pub fn mark_transaction_validated(&mut self) {
        self.ir_state.validation_scope = PathBuf::from(".");
        if let Some(tx) = self.ir_state.active_transaction.as_mut() {
            tx.validated = true;
            tx.latest_build_ok = Some(true);
        }
        self.ir_state.next_allowed_actions = vec![ActionKind::Refactor, ActionKind::Analyze];
    }

    pub fn rollback_current_transaction(&mut self) {
        self.ir_state.active_transaction = None;
        self.ir_state.current_target = Some(PathBuf::from("."));
        self.last_target = Some(PathBuf::from("."));
        self.ir_state.next_allowed_actions = vec![
            ActionKind::CodingPreview,
            ActionKind::Analyze,
            ActionKind::Refactor,
        ];
    }

    pub fn clear_transaction_for_new_target(&mut self, target: &PathBuf) {
        let should_clear = self
            .ir_state
            .active_transaction
            .as_ref()
            .map(|tx| tx.canonical_target != *target && (tx.validated || tx.applied))
            .unwrap_or(false);
        if should_clear {
            self.ir_state.active_transaction = None;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransactionLifecycleStep {
    Preview {
        target: PathBuf,
    },
    Apply {
        latest_diff_ref: Option<SessionAppliedDiff>,
    },
    Validate,
    Rollback,
    AnalyzeNewTarget {
        target: PathBuf,
    },
    RefactorContinuation {
        target: PathBuf,
    },
}

#[derive(Clone, Debug, Default)]
pub struct IRStateStore {
    pub state: IRState,
}

impl IRStateStore {
    pub fn rebuild_state(steps: &[TransactionLifecycleStep]) -> IRState {
        let mut store = Self::default();
        for step in steps {
            match step {
                TransactionLifecycleStep::Preview { target } => {
                    store.state.current_target = Some(target.clone());
                    store.state.active_transaction = Some(IRActiveTransaction {
                        transaction_id: format!("tx:{}", target.display()),
                        canonical_target: target.clone(),
                        pending: true,
                        applied: false,
                        validated: false,
                        rollback_available: false,
                        latest_diff_ref: None,
                        latest_build_ok: None,
                        file_hash: None,
                    });
                    store.state.next_allowed_actions =
                        vec![ActionKind::Apply, ActionKind::Refactor, ActionKind::Analyze];
                }
                TransactionLifecycleStep::Apply { latest_diff_ref } => {
                    if let Some(tx) = store.state.active_transaction.as_mut() {
                        tx.pending = false;
                        tx.applied = true;
                        tx.rollback_available = true;
                        tx.latest_diff_ref = latest_diff_ref.clone();
                    }
                    store.state.next_allowed_actions = vec![
                        ActionKind::Validate,
                        ActionKind::Refactor,
                        ActionKind::Rollback,
                    ];
                }
                TransactionLifecycleStep::Validate => {
                    store.state.validation_scope = PathBuf::from(".");
                    if let Some(tx) = store.state.active_transaction.as_mut() {
                        tx.validated = true;
                        tx.latest_build_ok = Some(true);
                    }
                    store.state.next_allowed_actions =
                        vec![ActionKind::Refactor, ActionKind::Analyze];
                }
                TransactionLifecycleStep::Rollback => {
                    store.state.active_transaction = None;
                    store.state.current_target = Some(PathBuf::from("."));
                    store.state.next_allowed_actions = vec![
                        ActionKind::CodingPreview,
                        ActionKind::Analyze,
                        ActionKind::Refactor,
                    ];
                }
                TransactionLifecycleStep::AnalyzeNewTarget { target }
                | TransactionLifecycleStep::RefactorContinuation { target } => {
                    let clear = store
                        .state
                        .active_transaction
                        .as_ref()
                        .map(|tx| tx.canonical_target != *target && (tx.validated || tx.applied))
                        .unwrap_or(false);
                    if clear {
                        store.state.active_transaction = None;
                    }
                    store.state.current_target = Some(target.clone());
                }
            }
        }
        store.state
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{IRStateStore, TransactionLifecycleStep};
    use crate::service::dto::ActionKind;

    #[test]
    fn ir_next_actions_match_transition_rules() {
        let state = IRStateStore::rebuild_state(&[
            TransactionLifecycleStep::Preview {
                target: PathBuf::from("src/repl.rs"),
            },
            TransactionLifecycleStep::Apply {
                latest_diff_ref: None,
            },
            TransactionLifecycleStep::Validate,
        ]);

        assert_eq!(
            state.next_allowed_actions,
            vec![ActionKind::Refactor, ActionKind::Analyze]
        );
        let tx = state.active_transaction.expect("active transaction");
        assert!(tx.applied);
        assert!(tx.validated);
        assert_eq!(tx.latest_build_ok, Some(true));
    }

    #[test]
    fn analyze_new_target_after_validate_starts_cleanly() {
        let state = IRStateStore::rebuild_state(&[
            TransactionLifecycleStep::Preview {
                target: PathBuf::from("src/repl.rs"),
            },
            TransactionLifecycleStep::Apply {
                latest_diff_ref: None,
            },
            TransactionLifecycleStep::Validate,
            TransactionLifecycleStep::AnalyzeNewTarget {
                target: PathBuf::from("src/new_target.rs"),
            },
        ]);

        assert!(state.active_transaction.is_none());
        assert_eq!(
            state.current_target,
            Some(PathBuf::from("src/new_target.rs"))
        );
    }
}
