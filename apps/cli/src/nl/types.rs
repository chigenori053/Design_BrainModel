use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntentType {
    RulesLearn,
    RulesList,
    CodingEdit,
    AnalyzeArchitecture,
    Analyze,
    Coding,
    Validate,
    StructureView,
    StructureEdit,
    StructureDiff,
    StructureDispatch,
    StructureUndo,
    StructureRedo,
    Run,
    Rules,
    Memory,
    GitCommit,
    GitPR,
    AlternativeMutationSearch,
    DesignDeltaReasoning,
    ExplainDesignTradeoff,
    MetaPlannerEdit,
    Repair,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SupportedLanguage {
    Japanese,
    English,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedTarget {
    pub path: PathBuf,
    pub node: Option<String>,
    pub scope: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodingOptions {
    pub safe: bool,
    pub check: bool,
    pub request: Option<String>,
}

impl Default for CodingOptions {
    fn default() -> Self {
        Self {
            safe: true,
            check: true,
            request: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefactorSpec {
    pub target: PathBuf,
    pub request: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepairSpec {
    pub target: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlannedStep {
    Analyze(PathBuf),
    Coding(PathBuf, CodingOptions),
    Validate(PathBuf),
    StructureView(PathBuf),
    StructureEdit(PathBuf),
    StructureDiff(PathBuf, Option<String>),
    StructureUndo(PathBuf),
    StructureRedo(PathBuf),
    Run(PathBuf),
    Rules,
    Memory(PathBuf),
    GitCommit(PathBuf),
    GitPR(PathBuf),
    AlternativeMutationSearch(String),
    DesignDeltaReasoning(String),
    ExplainDesignTradeoff(String),
    ApplyPreviousCodingStep,
    RollbackCurrentTransaction,
    IrReload(PathBuf),
    IrReloadAll(PathBuf),
    ShowDeps(PathBuf),
    Refactor(RefactorSpec),
    Repair(RepairSpec),
    Apply,
    Reload,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentScope {
    Workspace,
    Target(PathBuf),
    Node(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CodingIntent {
    FixBug {
        target: PathBuf,
        description: String,
    },
    Refactor {
        scope: IntentScope,
    },
    AddFeature {
        target: PathBuf,
        spec: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CommandPlan {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent: Option<CodingIntent>,
    pub steps: Vec<PlannedStep>,
}

// ── ExecutionPlan（DBM-PLAN-EXEC-STRUCT-SPEC v1.0） ───────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operation {
    Analyze,
    Refactor,
    Validate,
    Composite(Vec<Operation>),
    Apply,
    Rollback,
    Reload,
    Repair,
    NoOp,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanArgs {
    pub query: Option<String>,
    pub flags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanSource {
    ReplInput,
    FileRoute,
    System,
}

impl Default for PlanSource {
    fn default() -> Self {
        PlanSource::ReplInput
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanMetadata {
    pub source: PlanSource,
    pub timestamp: u64,
}

impl Default for PlanMetadata {
    fn default() -> Self {
        Self {
            source: PlanSource::default(),
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

impl PlanMetadata {
    pub fn with_source(source: PlanSource) -> Self {
        Self {
            source,
            ..Self::default()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationPolicy {
    Strict,
    Advisory,
}

impl Default for ValidationPolicy {
    fn default() -> Self {
        Self::Strict
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStage {
    Plan,
    Validate,
    Execute,
    PostValidate,
    Commit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionMetadata {
    pub deterministic: bool,
    pub rollback_required: bool,
    pub audit_enabled: bool,
}

impl Default for ExecutionMetadata {
    fn default() -> Self {
        Self {
            deterministic: true,
            rollback_required: true,
            audit_enabled: true,
        }
    }
}

/// Plannerの出力、Executorの入力となる中核データ構造。
/// 文字列・JSONは一切含まない。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionPlan {
    pub operation: Operation,
    pub target: Option<PathBuf>,
    pub args: PlanArgs,
    pub metadata: PlanMetadata,
    pub validation_policy: ValidationPolicy,
    pub execution_stages: Vec<ExecutionStage>,
    pub execution_metadata: ExecutionMetadata,
}

impl ExecutionPlan {
    pub fn new(operation: Operation, target: Option<PathBuf>, source: PlanSource) -> Self {
        Self {
            operation,
            target,
            args: PlanArgs::default(),
            metadata: PlanMetadata::with_source(source),
            validation_policy: ValidationPolicy::default(),
            execution_stages: default_execution_stages(),
            execution_metadata: ExecutionMetadata::default(),
        }
    }

    pub fn noop() -> Self {
        Self::new(Operation::NoOp, None, PlanSource::System)
    }

    pub fn with_query(mut self, query: impl Into<String>) -> Self {
        self.args.query = Some(query.into());
        self
    }

    pub fn has_effect(&self) -> bool {
        match &self.operation {
            Operation::NoOp => false,
            Operation::Composite(ops) => ops.iter().any(|op| !matches!(op, Operation::NoOp)),
            Operation::Refactor
            | Operation::Analyze
            | Operation::Validate
            | Operation::Apply
            | Operation::Rollback
            | Operation::Reload
            | Operation::Repair => true,
        }
    }
}

fn default_execution_stages() -> Vec<ExecutionStage> {
    vec![
        ExecutionStage::Plan,
        ExecutionStage::Validate,
        ExecutionStage::Execute,
        ExecutionStage::PostValidate,
    ]
}

impl From<PlannedStep> for ExecutionPlan {
    fn from(step: PlannedStep) -> Self {
        match step {
            PlannedStep::Analyze(path) => {
                ExecutionPlan::new(Operation::Analyze, Some(path), PlanSource::System)
            }
            PlannedStep::Refactor(spec) => ExecutionPlan {
                operation: Operation::Refactor,
                target: Some(spec.target),
                args: PlanArgs {
                    query: Some(spec.request),
                    flags: vec![],
                },
                metadata: PlanMetadata::default(),
                validation_policy: ValidationPolicy::default(),
                execution_stages: default_execution_stages(),
                execution_metadata: ExecutionMetadata::default(),
            },
            PlannedStep::Repair(spec) => {
                ExecutionPlan::new(Operation::Repair, Some(spec.target), PlanSource::System)
            }
            PlannedStep::Validate(path) => {
                ExecutionPlan::new(Operation::Validate, Some(path), PlanSource::System)
            }
            PlannedStep::Apply => ExecutionPlan::new(Operation::Apply, None, PlanSource::System),
            PlannedStep::RollbackCurrentTransaction => {
                ExecutionPlan::new(Operation::Rollback, None, PlanSource::System)
            }
            PlannedStep::Reload => ExecutionPlan::new(Operation::Reload, None, PlanSource::System),
            PlannedStep::IrReload(path) => {
                ExecutionPlan::new(Operation::Reload, Some(path), PlanSource::System)
            }
            PlannedStep::IrReloadAll(path) => {
                ExecutionPlan::new(Operation::Reload, Some(path), PlanSource::System)
            }
            _ => ExecutionPlan::noop(),
        }
    }
}

impl From<CommandPlan> for ExecutionPlan {
    fn from(plan: CommandPlan) -> Self {
        plan.steps
            .into_iter()
            .next()
            .map(ExecutionPlan::from)
            .unwrap_or_else(ExecutionPlan::noop)
    }
}

/// ExecutionPlan → CommandPlan（IR内部ストレージ・mlaal互換レイヤー用）
impl From<&ExecutionPlan> for CommandPlan {
    fn from(plan: &ExecutionPlan) -> Self {
        let target = plan.target.clone().unwrap_or_else(|| PathBuf::from("."));
        let step = match &plan.operation {
            Operation::Analyze => PlannedStep::Analyze(target),
            Operation::Refactor => PlannedStep::Refactor(RefactorSpec {
                target,
                request: plan.args.query.clone().unwrap_or_default(),
            }),
            Operation::Validate => PlannedStep::Validate(target),
            Operation::Composite(ops) => {
                let Some(first) = ops.first() else {
                    return CommandPlan::default();
                };
                return CommandPlan::from(&ExecutionPlan {
                    operation: first.clone(),
                    target: plan.target.clone(),
                    args: plan.args.clone(),
                    metadata: plan.metadata.clone(),
                    validation_policy: plan.validation_policy,
                    execution_stages: plan.execution_stages.clone(),
                    execution_metadata: plan.execution_metadata.clone(),
                });
            }
            Operation::Apply => PlannedStep::Apply,
            Operation::Rollback => PlannedStep::RollbackCurrentTransaction,
            Operation::Reload => PlannedStep::Reload,
            Operation::Repair => PlannedStep::Repair(RepairSpec { target }),
            Operation::NoOp => return CommandPlan::default(),
        };
        CommandPlan {
            intent: None,
            steps: vec![step],
        }
    }
}
