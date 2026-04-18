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
    /// R1: previous coding dry-run transaction を apply へ昇格するステップ。
    /// generic planner を bypass し、前回 checked && !applied の transaction を再利用する。
    ApplyPreviousCodingStep,
    /// Explicit IR rollback transition for the active REPL transaction lifecycle.
    RollbackCurrentTransaction,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CommandPlan {
    pub steps: Vec<PlannedStep>,
}
