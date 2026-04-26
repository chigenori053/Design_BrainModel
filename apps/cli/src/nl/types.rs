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
    /// Phase 4 (DBM-IR-SYNC-SPEC v1.0): re-sync the IR with the on-disk file.
    ///
    /// Clears any pending (unapplied) transaction / diff so that the next
    /// `refactor` command reads a fresh snapshot.  Emits telemetry that
    /// includes the current file hash and drift status.
    IrReload(PathBuf),
    /// Phase B-3: reload the whole Project IR graph.
    IrReloadAll(PathBuf),
    /// Phase B-3: render dependency edges for a tracked file.
    ShowDeps(PathBuf),
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
