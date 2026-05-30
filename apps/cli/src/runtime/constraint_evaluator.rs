use crate::nl::language_core_ir_adapter::{IrAction, IrIntentRequest};
use crate::nl::context_aware_plan_target_resolver::RuntimeConstraint;

/// 制約評価の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstraintDecision {
    /// 実行許可
    Allow,
    /// 実行拒否
    Reject {
        /// 拒否理由（例: "NoApplyConstraint"）
        reason: String,
    },
}

/// 実行時制約の評価器。
/// Spec DBM-CONSTRAINT-ENFORCEMENT-SPEC v1.0
pub struct ConstraintEvaluator;

impl ConstraintEvaluator {
    /// 自然言語インテント (IR レベル) を評価する。
    pub fn evaluate_ir_request(
        ir_request: &IrIntentRequest,
        constraints: &RuntimeConstraint,
    ) -> ConstraintDecision {
        let action = &ir_request.action;

        // 1. NoApply 制約
        if constraints.no_apply {
            if matches!(
                action,
                IrAction::Apply
                    | IrAction::ModifyFile
                    | IrAction::Refactor
                    | IrAction::GenerateChangePlan
                    | IrAction::ReviewValidatedPlan
                    | IrAction::ValidatePlan
            ) {
                return ConstraintDecision::Reject {
                    reason: "NoApplyConstraint".to_string(),
                };
            }
        }

        // 2. NoModify 制約
        if constraints.no_modify {
            if matches!(action, IrAction::ModifyFile | IrAction::Refactor) {
                return ConstraintDecision::Reject {
                    reason: "NoModifyConstraint".to_string(),
                };
            }
        }

        // 3. NoDelete 制約
        if constraints.no_delete {
            let lower = ir_request.raw_input.to_lowercase();
            if lower.contains("削除") || lower.contains("delete") || lower.contains("remove") {
                if matches!(action, IrAction::ModifyFile | IrAction::Refactor) {
                    return ConstraintDecision::Reject {
                        reason: "NoDeleteConstraint".to_string(),
                    };
                }
            }
        }

        ConstraintDecision::Allow
    }

    /// Git 操作を評価する。
    pub fn evaluate_git(constraints: &RuntimeConstraint) -> ConstraintDecision {
        if constraints.no_git_operation {
            ConstraintDecision::Reject {
                reason: "NoGitConstraint".to_string(),
            }
        } else {
            ConstraintDecision::Allow
        }
    }

    /// 外部コマンド実行を評価する。
    pub fn evaluate_external(_command: &str, constraints: &RuntimeConstraint) -> ConstraintDecision {
        if constraints.no_external_command {
            // 本 SPEC に基づき、shell/cargo/rustc 等の外部実行を拒否する
            // (内部的な読み取りコマンド等を除外する必要がある場合はここで判定する)
            ConstraintDecision::Reject {
                reason: "NoExternalCommandConstraint".to_string(),
            }
        } else {
            ConstraintDecision::Allow
        }
    }
}
