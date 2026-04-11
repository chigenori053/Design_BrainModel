use std::path::PathBuf;
use std::{error::Error, fmt};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::state::{LoopEntryState, PatchStrategy};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum LoopOrigin {
    Analyze,
    Coding,
    Validate,
    Structure,
    MemoryRecall,
    PreviousTransaction,
    SubcommandBridge,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepairLoopContext {
    pub target: Option<PathBuf>,
    pub logical_node: Option<String>,
    pub changed_files: Vec<PathBuf>,
    pub diagnostics: Vec<String>,
    pub rollback_token: Option<String>,
    pub previous_strategy: Option<PatchStrategy>,
    pub origin: LoopOrigin,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PromotionGuard {
    pub require_explicit_scope: bool,
    pub require_diagnostics_for_retry: bool,
    pub require_rollback_for_write_origin: bool,
    pub min_memory_confidence: f32,
    pub require_unique_structure_binding: bool,
}

impl Default for PromotionGuard {
    fn default() -> Self {
        Self {
            require_explicit_scope: true,
            require_diagnostics_for_retry: true,
            require_rollback_for_write_origin: true,
            min_memory_confidence: 0.8,
            require_unique_structure_binding: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromotionError {
    AmbiguousTarget,
    MissingDiagnostics,
    MissingRollbackToken,
    NonUniqueSourceBinding,
    LowRecallConfidence,
}

impl fmt::Display for PromotionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            PromotionError::AmbiguousTarget => "ambiguous target",
            PromotionError::MissingDiagnostics => "missing diagnostics",
            PromotionError::MissingRollbackToken => "missing rollback token",
            PromotionError::NonUniqueSourceBinding => "non-unique source binding",
            PromotionError::LowRecallConfidence => "low recall confidence",
        };
        f.write_str(message)
    }
}

impl Error for PromotionError {}

pub trait LoopPromotable {
    fn promote(self) -> Result<RepairLoopContext>;
}

impl RepairLoopContext {
    pub fn validate_with_guard(&self, guard: PromotionGuard) -> Result<()> {
        match self.origin {
            LoopOrigin::Analyze => {
                if guard.require_explicit_scope && self.target.is_none() {
                    return Err(PromotionError::AmbiguousTarget.into());
                }
            }
            LoopOrigin::Coding => {
                if guard.require_diagnostics_for_retry
                    && !self.diagnostics.is_empty()
                    && guard.require_rollback_for_write_origin
                    && self.rollback_token.is_none()
                {
                    return Err(PromotionError::MissingRollbackToken.into());
                }
            }
            LoopOrigin::Validate => {
                if guard.require_diagnostics_for_retry && self.diagnostics.is_empty() {
                    return Err(PromotionError::MissingDiagnostics.into());
                }
            }
            LoopOrigin::Structure => {
                if guard.require_unique_structure_binding && self.target.is_none() {
                    return Err(PromotionError::NonUniqueSourceBinding.into());
                }
            }
            LoopOrigin::MemoryRecall => {
                if self.previous_strategy.is_none() {
                    return Err(PromotionError::LowRecallConfidence.into());
                }
            }
            LoopOrigin::PreviousTransaction | LoopOrigin::SubcommandBridge => {}
        }
        Ok(())
    }

    pub fn suggested_entry_state(&self) -> Result<LoopEntryState> {
        match self.origin {
            LoopOrigin::Analyze => Ok(LoopEntryState::PlanPatch),
            LoopOrigin::Coding => {
                if !self.diagnostics.is_empty() {
                    if self.rollback_token.is_none() {
                        return Err(PromotionError::MissingRollbackToken.into());
                    }
                    Ok(LoopEntryState::RetryDecision)
                } else if !self.changed_files.is_empty() {
                    Ok(LoopEntryState::Verify)
                } else {
                    Err(PromotionError::MissingDiagnostics.into())
                }
            }
            LoopOrigin::Validate => {
                if self.diagnostics.is_empty() {
                    return Err(PromotionError::MissingDiagnostics.into());
                }
                Ok(LoopEntryState::RetryDecision)
            }
            LoopOrigin::Structure => Ok(LoopEntryState::Analyze),
            LoopOrigin::MemoryRecall => Ok(LoopEntryState::PlanPatch),
            LoopOrigin::PreviousTransaction | LoopOrigin::SubcommandBridge => {
                if !self.diagnostics.is_empty() {
                    Ok(LoopEntryState::RetryDecision)
                } else if !self.changed_files.is_empty() {
                    Ok(LoopEntryState::Verify)
                } else {
                    Ok(LoopEntryState::Analyze)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct DummyPromotable;

    impl LoopPromotable for DummyPromotable {
        fn promote(self) -> Result<RepairLoopContext> {
            Ok(RepairLoopContext {
                target: Some(PathBuf::from("apps/cli/src/repl.rs")),
                logical_node: Some("determinism".to_string()),
                changed_files: vec![PathBuf::from("apps/cli/src/repl.rs")],
                diagnostics: vec![String::from("ok")],
                rollback_token: Some("rb-1".to_string()),
                previous_strategy: Some(PatchStrategy::ImportRebind),
                origin: LoopOrigin::Analyze,
            })
        }
    }

    fn consume_promotable<T: LoopPromotable>(value: T) -> Result<RepairLoopContext> {
        value.promote()
    }

    #[test]
    fn repair_loop_context_constructs_compile_safely() {
        let context = RepairLoopContext {
            target: None,
            logical_node: None,
            changed_files: Vec::new(),
            diagnostics: Vec::new(),
            rollback_token: None,
            previous_strategy: None,
            origin: LoopOrigin::Validate,
        };
        assert_eq!(context.origin, LoopOrigin::Validate);
        assert!(context.target.is_none());
        assert!(context.logical_node.is_none());
    }

    #[test]
    fn all_loop_origin_variants_construct() {
        let origins = [
            LoopOrigin::Analyze,
            LoopOrigin::Coding,
            LoopOrigin::Validate,
            LoopOrigin::Structure,
            LoopOrigin::MemoryRecall,
            LoopOrigin::PreviousTransaction,
            LoopOrigin::SubcommandBridge,
        ];
        assert_eq!(origins.len(), 7);
    }

    #[test]
    fn loop_promotable_is_generic_not_trait_object_dependent() {
        let promoted = consume_promotable(DummyPromotable).expect("promotion should succeed");
        assert_eq!(promoted.origin, LoopOrigin::Analyze);
        assert_eq!(
            promoted.previous_strategy,
            Some(PatchStrategy::ImportRebind)
        );
    }

    #[test]
    fn exports_are_available_via_nl_loop() {
        let context = crate::nl::r#loop::RepairLoopContext {
            target: None,
            logical_node: Some("replay".to_string()),
            changed_files: Vec::new(),
            diagnostics: Vec::new(),
            rollback_token: None,
            previous_strategy: None,
            origin: crate::nl::r#loop::LoopOrigin::Structure,
        };
        assert_eq!(context.origin, crate::nl::r#loop::LoopOrigin::Structure);
    }

    #[test]
    fn coding_context_selects_retry_when_diagnostics_exist() {
        let context = RepairLoopContext {
            target: Some(PathBuf::from("apps/cli/src/repl.rs")),
            logical_node: None,
            changed_files: Vec::new(),
            diagnostics: vec![String::from("cargo check failed")],
            rollback_token: Some("rb-1".to_string()),
            previous_strategy: None,
            origin: LoopOrigin::Coding,
        };
        assert_eq!(
            context.suggested_entry_state().unwrap(),
            LoopEntryState::RetryDecision
        );
    }

    #[test]
    fn missing_rollback_guard_rejects_write_origin_retry() {
        let context = RepairLoopContext {
            target: Some(PathBuf::from("apps/cli/src/repl.rs")),
            logical_node: None,
            changed_files: vec![PathBuf::from("apps/cli/src/repl.rs")],
            diagnostics: vec![String::from("cargo check failed")],
            rollback_token: None,
            previous_strategy: None,
            origin: LoopOrigin::Coding,
        };
        let error = context
            .validate_with_guard(PromotionGuard::default())
            .expect_err("coding retry without rollback token must fail");
        assert!(error.to_string().contains("missing rollback token"));
    }
}
