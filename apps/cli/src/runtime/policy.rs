use std::collections::HashSet;
use std::fmt;
use crate::nl::language_core_ir_adapter::{IrAction, IrIntentRequest};
use crate::nl::context_aware_plan_target_resolver::RuntimeConstraint;

/// ユーザーの役割。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PolicyRole {
    Reviewer,
    Developer,
    Operator,
    AutonomousAgent,
}

impl fmt::Display for PolicyRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reviewer => write!(f, "Reviewer"),
            Self::Developer => write!(f, "Developer"),
            Self::Operator => write!(f, "Operator"),
            Self::AutonomousAgent => write!(f, "AutonomousAgent"),
        }
    }
}

/// 実行権限。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Permission {
    Analyze,
    Modify,
    Apply,
    Delete,
    Git,
    ExternalCommand,
}

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Analyze => write!(f, "Analyze"),
            Self::Modify => write!(f, "Modify"),
            Self::Apply => write!(f, "Apply"),
            Self::Delete => write!(f, "Delete"),
            Self::Git => write!(f, "Git"),
            Self::ExternalCommand => write!(f, "ExternalCommand"),
        }
    }
}

/// ポリシープロファイル。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PolicyProfile {
    pub role: PolicyRole,
    pub permissions: HashSet<Permission>,
}

impl PolicyProfile {
    /// デフォルトのポリシー（Developer）を作成する。
    pub fn default_developer() -> Self {
        let mut permissions = HashSet::new();
        permissions.insert(Permission::Analyze);
        permissions.insert(Permission::Modify);
        Self {
            role: PolicyRole::Developer,
            permissions,
        }
    }

    /// 指定した役割の標準ポリシーを作成する。
    pub fn from_role(role: PolicyRole) -> Self {
        let mut permissions = HashSet::new();
        permissions.insert(Permission::Analyze);

        match role {
            PolicyRole::Reviewer => {
                // Analyze のみ
            }
            PolicyRole::Developer => {
                permissions.insert(Permission::Modify);
            }
            PolicyRole::Operator => {
                permissions.insert(Permission::Modify);
                permissions.insert(Permission::Apply);
                permissions.insert(Permission::Delete);
                permissions.insert(Permission::Git);
                permissions.insert(Permission::ExternalCommand);
            }
            PolicyRole::AutonomousAgent => {
                permissions.insert(Permission::Modify);
                // Apply は条件付き（△）だが、現状は許可リストに入れないか、
                // あるいは別の仕組みで制御する。SPEC v1.0 では Modify まで。
            }
        }

        Self { role, permissions }
    }

    /// 既存の RuntimeConstraint を適用して権限を制限する（互換モード）。
    pub fn apply_constraints(&mut self, constraints: &RuntimeConstraint) {
        if constraints.no_apply {
            self.permissions.remove(&Permission::Apply);
        }
        if constraints.no_modify {
            self.permissions.remove(&Permission::Modify);
        }
        if constraints.no_delete {
            self.permissions.remove(&Permission::Delete);
        }
        if constraints.no_git_operation {
            self.permissions.remove(&Permission::Git);
        }
        if constraints.no_external_command {
            self.permissions.remove(&Permission::ExternalCommand);
        }
    }
}

/// ポリシー評価の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PolicyDecision {
    Allow,
    Reject { reason: String },
}

/// ポリシー評価器。
/// Spec DBM-POLICY-LAYER-SPEC v1.0
pub struct PolicyEvaluator;

impl PolicyEvaluator {
    /// 自然言語インテント (IR レベル) を評価する。
    pub fn evaluate_ir_request(
        ir_request: &IrIntentRequest,
        profile: &PolicyProfile,
    ) -> PolicyDecision {
        let action = &ir_request.action;
        
        let required_permission = match action {
            IrAction::AnalyzeProject
            | IrAction::AnalyzeWorkspace
            | IrAction::AnalyzeDependencies
            | IrAction::AnalyzeModuleStructure
            | IrAction::AnalyzeFile
            | IrAction::AnalyzeSymbol
            | IrAction::AnalyzeTests
            | IrAction::AnalyzeDeadTests
            | IrAction::AnalyzeRegressionTests
            | IrAction::AnalyzeStructuralProblems
            | IrAction::AnalyzeSpecification => Permission::Analyze,

            IrAction::ModifyFile | IrAction::Refactor | IrAction::GenerateChangePlan => {
                // Delete 操作が含まれる場合は Delete 権限が必要
                let lower = ir_request.raw_input.to_lowercase();
                if lower.contains("削除") || lower.contains("delete") || lower.contains("remove") {
                    Permission::Delete
                } else {
                    Permission::Modify
                }
            }

            IrAction::Apply => Permission::Apply,
            
            IrAction::ValidatePlan | IrAction::ReviewValidatedPlan | IrAction::ReviewSafety => {
                // これらは Apply の前段階だが、便宜上 Analyze または専用権限が必要
                // SPEC では明示されていないが、ReadOnly 的なので Analyze 扱いとする
                Permission::Analyze
            }

            IrAction::Constraint => Permission::Analyze, // 制約設定自体は誰でも可能（あるいは専用権限）
            IrAction::Unknown => Permission::Analyze,
        };

        if profile.permissions.contains(&required_permission) {
            PolicyDecision::Allow
        } else {
            PolicyDecision::Reject {
                reason: format!("PermissionDenied: Role {} requires {} permission for action {}", 
                    profile.role, required_permission, action),
            }
        }
    }

    /// Git 操作を評価する。
    pub fn evaluate_git(profile: &PolicyProfile) -> PolicyDecision {
        if profile.permissions.contains(&Permission::Git) {
            PolicyDecision::Allow
        } else {
            PolicyDecision::Reject {
                reason: format!("PermissionDenied: Role {} requires Git permission", profile.role),
            }
        }
    }

    /// 外部コマンド実行を評価する。
    pub fn evaluate_external(command: &str, profile: &PolicyProfile) -> PolicyDecision {
        if profile.permissions.contains(&Permission::ExternalCommand) {
            PolicyDecision::Allow
        } else {
            PolicyDecision::Reject {
                reason: format!("PermissionDenied: Role {} requires ExternalCommand permission for '{}'", 
                    profile.role, command),
            }
        }
    }
}
