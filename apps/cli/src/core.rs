use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use design_search_engine::stable_v03::DeterministicBeamSearchEngine;
use memory_engine::InMemoryEngine;
use runtime_core::{CoreRuntime, RuntimeExecutionResult};
use serde_json::json;
use sha2::{Digest, Sha256};
use strategy_engine::{
    Action, DryRunIntegrator, ExecutionContext as StrategyExecutionContext, ExecutionHistory,
    ExecutionPlanCandidate, Intent, Limits, ResolvedTarget, StrategyEngine, StrategyInput,
    StrategyOutput, generate_candidates_from_intent_with_limits, requires_clarification,
};

use std::sync::atomic::{AtomicU64, Ordering};

use crate::capability::{
    CodeAnalysisResult, MemoryAnalysisResult, OutputTypeId, ProjectStructureAnalysisResult,
    RuntimeAnalyzeDispatcher, TestInventoryResult,
};
use crate::coding::{
    ChangeSummary, ChangeType, CodeChange, CodeChangeSet, CodingOptions, DiffHunk,
    execute_code_change_set,
};
use crate::command::CommandRegistry;
use crate::commands::register_defaults;
use crate::git::commands::GitCommand;
use crate::git::dirty_tree::{
    DirtyTreePolicy, reject_dirty_worktree_except, reject_unstaged_worktree,
};
use crate::git::executor::{
    add_file as git_add_file, commit_fixed as git_commit_fixed, execute_read as git_execute_read,
};
use crate::git::policy::{CommandType, classify as git_command_policy};
use crate::git::telemetry::{GitExecutionRecord, git_record_json, transaction_record_json};
use crate::git::transaction::{ExecutionTransaction, GitPhase};
use crate::nl::context_aware_plan_target_resolver::{
    ChangePlanCandidate, NarrowTarget, PlanValidationResult, PreviousAnalysisContext,
    PreviousValidationContext, ReplSessionContext, SelectedCandidateContext, ValidatedPlanContext,
    ValidationStatus, stable_context_hash,
};
use crate::nl::language_core_ir_adapter::{ExecutionMode, IrAction, IrTarget};
use crate::nl::normalization::{TargetResolutionFailure, target_only_input_resolution};
use crate::pipeline::PipelineState;
use crate::refactor::PatchScope;
use crate::state_graph::StateGraph;

static REQUEST_COUNTER: AtomicU64 = AtomicU64::new(1);
static SAFE_APPLY_TRANSACTION_COUNTER: AtomicU64 = AtomicU64::new(1);

fn next_request_id() -> u64 {
    REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed)
}

fn next_safe_apply_transaction_id() -> u64 {
    SAFE_APPLY_TRANSACTION_COUNTER.fetch_add(1, Ordering::Relaxed)
}

const ENABLE_OBSERVABILITY: bool = true;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreRequestKind {
    NaturalLanguage,
    SlashCommand,
    Followup,
    Apply,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreRequest {
    pub id: u64,
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InternalRequest {
    pub id: u64,
    pub input: String,
    pub kind: CoreRequestKind,
    pub context: ExecutionContext,
}

impl CoreRequest {
    pub fn new(raw: String) -> Self {
        Self {
            id: next_request_id(),
            raw,
        }
    }
}

const DESIGN_MAX_LINES: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasonUnit {
    pub id: String,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureTree {
    pub module: String,
    pub functions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Constraint {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignDocument {
    pub version: u64,
    pub reason_units: Vec<ReasonUnit>,
    pub structure: StructureTree,
    pub constraints: Vec<Constraint>,
    pub rendered: Vec<String>,
}

impl DesignDocument {
    pub fn new(
        version: u64,
        reason_units: Vec<ReasonUnit>,
        structure: StructureTree,
        constraints: Vec<Constraint>,
    ) -> Self {
        let mut doc = Self {
            version,
            reason_units,
            structure,
            constraints,
            rendered: Vec::new(),
        };
        doc.regenerate_rendered();
        doc
    }

    /// Compute a quality score in [0, 1] for this design document.
    pub fn score(&self) -> f64 {
        let reason_score = (self.reason_units.len() as f64 / 8.0).min(0.5);
        let structure_score = if self.structure.functions.is_empty() {
            0.0
        } else {
            0.2
        };
        let constraint_score = (self.constraints.len() as f64 / 4.0).min(0.3);
        reason_score + structure_score + constraint_score
    }

    pub fn regenerate_rendered(&mut self) {
        let mut rendered = vec!["[DESIGN]".to_string(), String::new()];
        rendered.push(format!("Module: {}", self.structure.module));
        for function in &self.structure.functions {
            rendered.push(format!("- {function}"));
        }

        if !self.reason_units.is_empty() {
            rendered.push(String::new());
            rendered.push("Reason Units:".to_string());
            for unit in &self.reason_units {
                rendered.push(format!("- {}: {}", unit.title, unit.summary));
            }
        }

        if !self.constraints.is_empty() {
            rendered.push(String::new());
            rendered.push("Constraints:".to_string());
            for constraint in &self.constraints {
                rendered.push(format!("- {}", constraint.text));
            }
        }

        rendered.truncate(DESIGN_MAX_LINES);
        self.rendered = rendered;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionContext {
    pub working_dir: PathBuf,
    pub pipeline_state: PipelineState,
    pub design_snapshot: Option<DesignDocument>,
    /// Proposal candidates awaiting user selection.  Phase 1C.5 §5.3.
    pub current_proposals: Option<Vec<ExecutionPlanCandidate>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CoreResponse {
    pub events: Vec<CoreEvent>,
    pub status: ExecutionStatus,
    pub design: Option<DesignDocument>,
    /// Canonical state snapshot after this operation.  Phase 4.5.
    /// Set by navigation commands (undo/jump/replay) themselves.
    /// Set by all other commands via `push_and_attach_core_state`.
    pub core_state: Option<CoreState>,
}

/// Execution plan held inside `CoreState`.  Phase 4.5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorePlan {
    pub summary: String,
    pub steps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewSnapshot {
    pub candidate_id: usize,
    pub target: NarrowTarget,
    pub plan_hash: u64,
    pub preview_hash: u64,
    pub created_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    WorkspaceRoot,
    File(String),
    Module(String),
    Symbol(String),
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorkspaceRoot => write!(f, "WorkspaceRoot"),
            Self::File(path) => write!(f, "File({path})"),
            Self::Module(name) => write!(f, "Module({name})"),
            Self::Symbol(name) => write!(f, "Symbol({name})"),
        }
    }
}

impl From<NarrowTarget> for Target {
    fn from(target: NarrowTarget) -> Self {
        match target {
            NarrowTarget::File(path) => Self::File(path),
            NarrowTarget::Module(name) => Self::Module(name),
            NarrowTarget::Symbol(name) => Self::Symbol(name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedCandidate {
    pub candidate_id: usize,
    pub target: Target,
    pub plan_hash: u64,
    pub timestamp: u64,
}

impl From<SelectedCandidateContext> for SelectedCandidate {
    fn from(selected: SelectedCandidateContext) -> Self {
        Self {
            candidate_id: selected.candidate_id,
            target: selected.target.into(),
            plan_hash: selected.plan_hash,
            timestamp: selected.timestamp,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedPlan {
    pub plan_hash: u64,
    pub candidate_id: usize,
    pub target: Target,
    pub approved: bool,
    pub apply_allowed: bool,
    pub timestamp: u64,
}

impl From<ValidatedPlanContext> for ValidatedPlan {
    fn from(validated: ValidatedPlanContext) -> Self {
        Self {
            plan_hash: validated.plan_hash,
            candidate_id: validated.candidate_id,
            target: validated.target.into(),
            approved: validated.approved,
            apply_allowed: validated.apply_allowed,
            timestamp: validated.timestamp,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyGuardDecision {
    Allow {
        candidate_id: usize,
        target: Target,
        validated_plan_hash: u64,
    },
    Reject {
        reason: ApplyGuardRejectReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyGuardRejectReason {
    MissingValidatedPlan,
    MissingSelectedCandidate,
    MissingPreviewSnapshot,
    WorkspaceRootApplyForbidden,
    CandidateMismatch,
    PlanHashMismatch,
    TargetMismatch,
    PreviewNotActive,
    StaleValidatedPlan,
}

impl std::fmt::Display for ApplyGuardRejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyGuardInput {
    pub selected_candidate: Option<SelectedCandidate>,
    pub validated_plan: Option<ValidatedPlan>,
    pub preview_snapshot: Option<ApplyGuardPreviewSnapshot>,
    pub pipeline_state: PipelineState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyGuardPreviewSnapshot {
    pub candidate_id: usize,
    pub target: Target,
    pub plan_hash: u64,
}

impl From<PreviewSnapshot> for ApplyGuardPreviewSnapshot {
    fn from(snapshot: PreviewSnapshot) -> Self {
        Self {
            candidate_id: snapshot.candidate_id,
            target: snapshot.target.into(),
            plan_hash: snapshot.plan_hash,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeApplyTransaction {
    pub transaction_id: u64,
    pub candidate_id: usize,
    pub validated_plan_hash: u64,
    pub target: Target,
    pub target_path: PathBuf,
    pub pre_apply_checksum: u64,
    pub planned_diff_checksum: u64,
    pub rollback_snapshot: RollbackSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackSnapshot {
    pub target_path: PathBuf,
    pub original_contents: String,
    pub original_checksum: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyResult {
    Applied {
        transaction_id: u64,
        post_apply_checksum: u64,
    },
    RolledBack {
        transaction_id: u64,
        reason: ApplyFailureReason,
    },
    Rejected {
        reason: ApplyRejectReason,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyFailureReason {
    TargetMissing,
    ChecksumMismatch,
    DiffApplyFailed,
    PostValidationFailed,
    RollbackFailed,
}

impl std::fmt::Display for ApplyFailureReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyRejectReason {
    GuardRejected,
    MissingValidatedPlan,
    MissingSelectedCandidate,
    MissingPreview,
    WorkspaceRootApplyForbidden,
    UnsupportedTarget,
    ChecksumMismatch,
}

impl std::fmt::Display for ApplyRejectReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Canonical state snapshot — the Single Source of Truth.  Phase 4.5.
///
/// Each `execute()` call produces one new `CoreState` pushed to `History`.
/// The UI holds a read-only cache; only Core mutates the history.
#[derive(Debug, Clone, PartialEq)]
pub struct CoreState {
    /// Monotonically increasing version number assigned by `History::push`.
    pub version: u64,
    /// Current design snapshot; `None` before the first successful execution.
    pub design: Option<DesignDocument>,
    /// Active proposal candidates awaiting `select <n>`.
    pub proposals: Vec<ExecutionPlanCandidate>,
    /// Currently selected execution plan.
    pub current_plan: Option<CorePlan>,
    /// Most-recent file diff produced by the last execution.
    pub last_diff: Option<Diff>,
    /// Active preview identity.  Previewed is valid only while this exists
    /// and matches the selected or validated plan context.
    pub preview_snapshot: Option<PreviewSnapshot>,
    /// Current pipeline phase.
    pub status: PipelineState,
    /// Exploration depth.  Spec DBM-LIMITS-INTEGRATION-STEP3 §4.2.
    pub depth: usize,
    /// 前回の解析コンテキスト。
    pub previous_analysis_context: Option<PreviousAnalysisContext>,
    /// REPL セッション全体で保持する Intent/Target/Plan 文脈。
    pub session_context: ReplSessionContext,
}

impl Default for CoreState {
    fn default() -> Self {
        Self {
            version: 0,
            design: None,
            proposals: Vec::new(),
            current_plan: None,
            last_diff: None,
            preview_snapshot: None,
            status: PipelineState::Idle,
            depth: 0,
            previous_analysis_context: None,
            session_context: ReplSessionContext::default(),
        }
    }
}

/// Append-only history of `CoreState` snapshots.  Phase 4.5.
///
/// Invariants:
/// - `cursor` always points to a valid index in `states`.
/// - `version` is monotonically increasing within each linear run.
/// - `undo`        moves the cursor back without re-execution.
/// - `jump`        moves the cursor to any past version.
/// - `replay_from` truncates forward states and establishes a new branch root.
#[derive(Debug, Clone)]
pub struct History {
    states: Vec<CoreState>,
    cursor: usize,
    next_version: u64,
    limits: Limits,
}

impl Default for History {
    fn default() -> Self {
        Self {
            states: vec![CoreState::default()],
            cursor: 0,
            next_version: 1,
            limits: Limits::default(),
        }
    }
}

impl History {
    pub fn with_limits(limits: Limits) -> Self {
        Self {
            states: vec![CoreState::default()],
            cursor: 0,
            next_version: 1,
            limits,
        }
    }

    /// Append `state`, assign its version, and advance the cursor.
    ///
    /// Truncates any states ahead of `cursor` — an undo followed by a new
    /// operation creates a fresh branch (spec §11 "undo後に新操作").
    pub fn push(&mut self, mut state: CoreState) {
        if self.cursor + 1 < self.states.len() {
            self.states.truncate(self.cursor + 1);
        }
        state.version = self.next_version;
        self.next_version = self.next_version.saturating_add(1);
        self.states.push(state);
        self.cursor = self.states.len() - 1;
        self.trim_to_limit();
    }

    /// Snapshot at the current cursor position.
    pub fn current(&self) -> &CoreState {
        &self.states[self.cursor]
    }

    /// Move the cursor one step back and return the restored snapshot.
    /// Returns `None` when already at the root (nothing to undo).
    pub fn undo(&mut self) -> Option<CoreState> {
        if self.cursor == 0 {
            return None;
        }
        self.cursor -= 1;
        Some(self.states[self.cursor].clone())
    }

    fn replace_current(&mut self, state: CoreState) {
        self.states[self.cursor] = state;
    }

    /// Move the cursor to the snapshot whose `version == version`.
    /// Returns the restored snapshot, or `None` if not found.
    pub fn jump(&mut self, version: u64) -> Option<CoreState> {
        let idx = self.states.iter().position(|s| s.version == version)?;
        self.cursor = idx;
        Some(self.states[self.cursor].clone())
    }

    /// Reset to the snapshot at `version`, truncate the forward chain, and
    /// return the restored snapshot as the new branch root.
    pub fn replay_from(&mut self, version: u64) -> Option<CoreState> {
        let idx = self.states.iter().position(|s| s.version == version)?;
        self.cursor = idx;
        self.states.truncate(idx + 1);
        Some(self.states[self.cursor].clone())
    }

    pub fn contains_replay_target(&self, version: u64) -> bool {
        version != 0 && self.states.iter().any(|s| s.version == version)
    }

    pub fn replay_distance_to(&self, version: u64) -> Option<usize> {
        let idx = self.states.iter().position(|s| s.version == version)?;
        Some(self.cursor.abs_diff(idx))
    }

    /// Read-only view of all recorded snapshots (for display / tests).
    pub fn entries(&self) -> &[CoreState] {
        &self.states
    }

    /// Current cursor index.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    fn trim_to_limit(&mut self) {
        let max_history = self.limits.max_history.max(1);
        if self.states.len() <= max_history {
            return;
        }
        let overflow = self.states.len() - max_history;
        self.states.drain(..overflow);
        self.cursor = self.cursor.saturating_sub(overflow);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffChunk {
    pub old_line: Option<usize>,
    pub new_line: Option<usize>,
    pub old: Option<String>,
    pub new: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff {
    pub file: String,
    pub changes: Vec<DiffChunk>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CoreEvent {
    Thinking {
        summary: String,
    },
    Editing {
        target: String,
        action: String,
        reason: Option<String>,
    },
    Plan {
        steps: Vec<String>,
    },
    Execution {
        step: String,
    },
    Preview {
        diff: Vec<String>,
    },
    Diff {
        file: String,
        changes: Vec<DiffChunk>,
    },
    Result {
        message: String,
    },
    DesignUpdate {
        summary: String,
        score: f64,
    },
    DesignDiff {
        changes: Vec<String>,
    },
    ErrorRecovery {
        candidates: Vec<ExecutionPlanCandidate>,
    },
    Pipeline {
        state: String,
    },
    Next {
        actions: Vec<String>,
    },
    Error {
        message: String,
    },
    Debug {
        message: String,
    },
    /// Structured execution proposal.  Spec DBM-EXECUTION-CANDIDATE-SPEC §8.
    Proposal {
        candidates: Vec<ExecutionPlanCandidate>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    Idle,
    Proposed,
    Planned,
    Executed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayValidationFailure {
    InvalidTarget,
    ReplayLimitExceeded,
    ParseFailure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewConfirmation {
    Confirm,
    Reject,
    Cancel,
    Reconfirm,
}

fn execution_status_label(status: ExecutionStatus) -> &'static str {
    match status {
        ExecutionStatus::Executed => "Success",
        ExecutionStatus::Idle => "Idle",
        ExecutionStatus::Proposed => "Proposed",
        ExecutionStatus::Planned => "Planned",
        ExecutionStatus::Failed => "Failed",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TraceLevel {
    Off,
    Error,
    Basic,
    Full,
}

const TRACE_LEVEL: TraceLevel = TraceLevel::Full;

macro_rules! trace_ir {
    ($level:expr, $stage:expr, $data:expr) => {{
        if trace_enabled($level) {
            emit_core_log($stage, $data.to_string());
        }
    }};
}

fn emit_core_log(stage: &str, data: String) {
    if crate::runtime::logging::tui_logging_isolated()
        || crate::runtime::logging::tui_surface_active()
    {
        return;
    }
    let line = format!("[IR-TRACE][{stage}] {data}\n");
    let _ = std::io::Write::write_all(&mut std::io::stderr(), line.as_bytes());
}

pub(crate) fn observability_enabled() -> bool {
    ENABLE_OBSERVABILITY
        && !crate::runtime::logging::tui_logging_isolated()
        && !crate::runtime::logging::tui_surface_active()
}

pub trait CoreExecutor {
    fn execute(&self, request: CoreRequest) -> CoreResponse;
}

pub struct RuntimeCoreBridge {
    runtime: CoreRuntime,
    strategy: StrategyEngine,
    pending_files: Mutex<Vec<PendingFile>>,
    applied_files: Mutex<Vec<AppliedFile>>,
    /// Canonical state history.  Phase 4.5.
    history: Mutex<History>,
    /// State deduplication graph.  Spec DBM-GRAPH-INTEGRATION-STEP2.
    state_graph: Mutex<StateGraph>,
    limits: Limits,
    registry: CommandRegistry,
}

impl RuntimeCoreBridge {
    pub fn new(runtime: CoreRuntime, strategy: StrategyEngine) -> Self {
        Self::new_with_limits(runtime, strategy, Limits::default())
    }

    pub fn new_with_limits(runtime: CoreRuntime, strategy: StrategyEngine, limits: Limits) -> Self {
        let mut registry = CommandRegistry::new();
        register_defaults(&mut registry);
        Self {
            runtime,
            strategy,
            pending_files: Mutex::new(Vec::new()),
            applied_files: Mutex::new(Vec::new()),
            history: Mutex::new(History::with_limits(limits)),
            state_graph: Mutex::new(StateGraph::with_limits(limits)),
            limits,
            registry,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(
            CoreRuntime::new_with_defaults(
                Arc::new(InMemoryEngine::default()),
                Arc::new(DeterministicBeamSearchEngine::default()),
            ),
            StrategyEngine::default(),
        )
    }
}

impl Default for RuntimeCoreBridge {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl CoreExecutor for RuntimeCoreBridge {
    /// Dispatch the request and attach the canonical `CoreState` snapshot.
    ///
    /// Navigation commands (undo/jump/replay) set `core_state` themselves;
    /// all other commands receive a freshly pushed state from
    /// `push_and_attach_core_state`.
    fn execute(&self, request: CoreRequest) -> CoreResponse {
        let id = request.id;
        let raw_input = request.raw.clone();

        // §5.1 分類 (classification) - raw_input に対して1回のみ行う
        let has_context = {
            let history = self.history.lock().unwrap();
            !history.current().proposals.is_empty()
        };
        let (kind, reason) = crate::router::route(&raw_input, has_context);

        // §5.2 変換 (transform) - SlashCommand の場合のみ適用
        let (mapped_input, normalized_input) = if kind == CoreRequestKind::SlashCommand {
            let mapped = crate::router::map_slash_command(&raw_input);
            let normalized = if mapped.starts_with('/') {
                mapped.trim_start_matches('/').to_string()
            } else {
                mapped.clone()
            };
            (Some(mapped), normalized)
        } else {
            (None, raw_input.clone())
        };

        // Resolve context from SSOT (History)
        let current_state = {
            let history = self.history.lock().unwrap();
            history.current().clone()
        };
        current_state.session_context.trace_load();
        let context = ExecutionContext {
            working_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            pipeline_state: current_state.status.clone(),
            design_snapshot: current_state.design.clone(),
            current_proposals: Some(current_state.proposals.clone()),
        };

        if observability_enabled() {
            println!("[ROUTE][id={}] kind={:?} reason=\"{}\"", id, kind, reason);
            println!("[ROUTE][id={}] input=\"{}\"", id, request.raw);
            if let Some(ref mapped) = mapped_input {
                if *mapped != request.raw {
                    println!("[ROUTE][id={}] mapped=\"{}\"", id, mapped);
                }
                if normalized_input != *mapped {
                    println!("[ROUTE][id={}] normalized=\"{}\"", id, normalized_input);
                }
            }

            let history_len = self.history.lock().unwrap().entries().len();
            let state = self.history.lock().unwrap().current().clone();
            println!(
                "[CORE][id={}] enter depth={} history={}",
                id, state.depth, history_len
            );

            println!(
                "[STATE][id={}] followup={} previous_context_used={}",
                id,
                kind == CoreRequestKind::Followup,
                has_context
            );

            let graph_nodes = self.state_graph.lock().expect("graph lock").node_count();
            println!(
                "[STATE][id={}] depth={} history={} graph_nodes={}",
                id, state.depth, history_len, graph_nodes
            );
        }

        let normalized_lower = normalized_input.trim().to_ascii_lowercase();
        let is_navigation_command = matches!(normalized_lower.as_str(), "undo" | "replay")
            || normalized_lower.starts_with("replay ");

        if current_state.status == PipelineState::Previewed
            && !raw_input.trim().starts_with('/')
            && !is_navigation_command
        {
            let request = InternalRequest {
                id,
                input: raw_input.trim().to_string(),
                kind,
                context: context.clone(),
            };
            let response = self.route_preview_input(request);
            if observability_enabled() {
                println!("[CORE][id={}] exit status={:?}", id, response.status);
            }
            return response;
        }

        if let Some(response) =
            self.try_handle_command(&normalized_input, kind, &current_state, &context, id)
        {
            let response = if response.core_state.is_some() {
                response
            } else {
                self.push_and_attach_core_state(response, id)
            };
            if observability_enabled() {
                println!("[CORE][id={}] exit status={:?}", id, response.status);
            }
            return response;
        }

        let internal_request = InternalRequest {
            id,
            input: normalized_input,
            kind,
            context,
        };

        let response = self.execute_dispatch(internal_request);
        let response = if response.core_state.is_some() {
            response
        } else {
            self.push_and_attach_core_state(response, id)
        };

        if observability_enabled() {
            println!("[CORE][id={}] exit status={:?}", id, response.status);
        }

        response
    }
}

impl RuntimeCoreBridge {
    fn try_handle_command(
        &self,
        input: &str,
        kind: CoreRequestKind,
        state: &CoreState,
        context: &ExecutionContext,
        id: u64,
    ) -> Option<CoreResponse> {
        let input = input.trim();
        let lower = input.to_ascii_lowercase();
        let request = InternalRequest {
            id,
            input: input.to_string(),
            kind,
            context: context.clone(),
        };

        if let Some(response) = self.try_handle_priority_command(&lower, input, &request) {
            return Some(response);
        }

        match target_only_input_resolution(input) {
            Ok(Some(target)) => return Some(target_context_response(target, id)),
            Err(TargetResolutionFailure::ConfirmationTokenLike { .. }) => {
                return Some(self.execute_natural_language(request));
            }
            Ok(None) | Err(TargetResolutionFailure::Unresolved { .. }) => {}
        }

        let has_followup_context = kind == CoreRequestKind::Followup || !state.proposals.is_empty();
        if has_followup_context {
            match lower.as_str() {
                "undo" => return Some(self.undo(&request)),
                "replay" => return Some(self.replay(&request, None)),
                "reselect" => return Some(self.show_proposals(&request)),
                "compare" => return Some(self.compare_proposals(&request)),
                _ if lower.starts_with("replay ") => {
                    return Some(self.replay(&request, input.split_whitespace().nth(1)));
                }
                _ if lower.starts_with("select ") => {
                    return Some(self.select_candidate(&request, input));
                }
                _ => {}
            }
        }

        if let Some(git_route) = crate::routing::git_router::route_git_command(input) {
            return Some(match git_route {
                Ok(command) => self.execute_git_command(command, id, &context.working_dir),
                Err(err) => error_response("ExecutionRejected", &err, id),
            });
        }

        match lower.as_str() {
            "undo" => Some(self.undo(&request)),
            "replay" => Some(self.replay(&request, None)),
            _ if lower.starts_with("replay ") => {
                Some(self.replay(&request, input.split_whitespace().nth(1)))
            }
            _ if is_forbidden_command(&lower) => Some(error_response("SafetyViolation", input, id)),
            _ => None,
        }
    }

    fn try_handle_priority_command(
        &self,
        lower: &str,
        input: &str,
        request: &InternalRequest,
    ) -> Option<CoreResponse> {
        match lower {
            "preview" | "apply" | "y" | "yes" | "cancel" | "n" | "no" | "undo" | "replay" => {
                self.execute_pipeline_builtin(request)
            }
            _ if lower.starts_with("replay ") || lower.starts_with("select ") => {
                self.execute_pipeline_builtin(request)
            }
            _ if input.contains("--json")
                || input.contains("--event")
                || input.contains("--preview") =>
            {
                None
            }
            _ => None,
        }
    }

    fn execute_dispatch(&self, request: InternalRequest) -> CoreResponse {
        match request.kind {
            CoreRequestKind::SlashCommand => self.execute_command(&request),
            CoreRequestKind::Apply => {
                if let Some(response) = self.execute_pipeline_builtin(&request) {
                    response
                } else {
                    self.apply(&request)
                }
            }
            CoreRequestKind::Followup | CoreRequestKind::NaturalLanguage => {
                self.execute_natural_language(request)
            }
        }
    }

    fn execute_natural_language(&self, request: InternalRequest) -> CoreResponse {
        let id = request.id;
        if observability_enabled() {
            println!("[CODING][id={}] stage=analyze", id);
        }
        if self.current_depth() >= self.limits.max_depth {
            return self.max_depth_response();
        }
        let session_context_snapshot = {
            let history = self.history.lock().unwrap();
            history.current().session_context.clone()
        };

        let mut events = vec![
            CoreEvent::Thinking {
                summary: "refining natural language intent / 自然言語の意図を精製中".to_string(),
            },
            CoreEvent::Debug {
                message: format!(
                    "[IR-TRACE][CONTEXT_LOAD] previous_analysis_context={} previous_plan_context={} previous_validation_context={} selected_candidate={} validated_plan={}",
                    if session_context_snapshot.previous_analysis_context.is_some() {
                        "Some"
                    } else {
                        "None"
                    },
                    if session_context_snapshot.previous_plan_context.is_some() {
                        "Some"
                    } else {
                        "None"
                    },
                    if session_context_snapshot
                        .previous_validation_context
                        .is_some()
                    {
                        "Some"
                    } else {
                        "None"
                    },
                    if session_context_snapshot.selected_candidate.is_some() {
                        "Some"
                    } else {
                        "None"
                    },
                    if session_context_snapshot.validated_plan.is_some() {
                        "Some"
                    } else {
                        "None"
                    },
                ),
            },
            trace_event(
                "INTENT",
                json!({
                    "raw_input": request.input,
                    "pipeline_state": request.context.pipeline_state.label(),
                    "timestamp": timestamp_millis(),
                }),
            ),
            CoreEvent::Pipeline {
                state: request.context.pipeline_state.label().to_string(),
            },
        ];

        if is_plan_validation_intent(&request.input) {
            return self.validate_selected_plan(request, events);
        }

        // ── LanguageCoreToIrAdapter ──────────────────────────────────────────
        // DefaultIntentRefiner は非 ASCII 文字をストリップするため日本語入力が
        // EmptyInput → InvalidInput になる。Adapter で事前に意味分類し、
        // ReadOnly な Analyze 系 Intent は execute_from_text を経由しない。
        use crate::nl::context_aware_plan_target_resolver::{
            has_context_reference, is_plan_only_intent, resolve_plan_target,
        };
        use crate::nl::language_core_ir_adapter::{
            classify_language_core_intent, is_candidate_proposal_intent, language_core_to_ir,
        };

        let lc_intent = {
            // DBM-DOCUMENT-CLASSIFIER-SPEC v1.0 §8
            use crate::runtime::document_classifier::{DocumentClassifier, InputKind as DocInputKind};
            let doc_kind = DocumentClassifier::classify(&request.input);

            if observability_enabled() {
                println!(
                    "[DOCUMENT_CLASSIFIER] input_kind={} confidence=1.0",
                    doc_kind
                );
            }

            match doc_kind {
                DocInputKind::NaturalLanguage | DocInputKind::Command | DocInputKind::Unknown => {
                    // 通常の処理を続行
                    classify_language_core_intent(&request.input)
                }
                DocInputKind::MarkdownDocument
                | DocInputKind::JsonDocument
                | DocInputKind::LogDocument
                | DocInputKind::StructuredSpec => {
                    // LanguageCore への送信を禁止 (Rule 1-4)
                    events.push(CoreEvent::Result {
                        message: format!(
                            "# Document Detected ({})\n\nThe input was classified as a **{}**. DBM does not process document fragments as natural language instructions directly to avoid irrelevant clarifications.\n\nIf you intended to provide a command, please ensure it follows the correct syntax.",
                            doc_kind, doc_kind
                        ),
                    });
                    return CoreResponse {
                        events,
                        status: ExecutionStatus::Executed,
                        design: None,
                        core_state: Some(self.history.lock().unwrap().current().clone()),
                    };
                }
            }
        };

        let mut ir_request = language_core_to_ir(lc_intent, &request.input);
        let lower_input = request.input.to_lowercase();
        if session_context_snapshot.previous_analysis_context.is_some()
            && session_context_snapshot.previous_plan_context.is_none()
            && is_candidate_proposal_intent(&lower_input)
            && !matches!(ir_request.action, IrAction::GenerateChangePlan)
        {
            ir_request.action = IrAction::GenerateChangePlan;
            ir_request.mode = ExecutionMode::PlanOnly;
            ir_request.target = session_context_snapshot
                .previous_analysis_context
                .as_ref()
                .map(|ctx| ctx.target.clone())
                .unwrap_or(IrTarget::WorkspaceRoot);
            events.push(CoreEvent::Debug {
                message:
                    "[IR-TRACE][PRIMARY_INTENT] selected=GenerateChangePlan reason=AnalysisToCandidateProposal"
                        .to_string(),
            });
        }
        if let Some(crate::nl::normalization::TargetResolutionFailure::ConfirmationTokenLike {
            raw,
        }) = ir_request.target_failure.clone()
        {
            events.push(CoreEvent::Debug {
                message: format!(
                    "[IR-TRACE][TARGET_REJECTED] target={raw} reason=ConfirmationTokenLike"
                ),
            });
            let original_raw_input = ir_request.raw_input.clone();
            let original_confidence = ir_request.confidence;
            ir_request = fallback_for_confirmation_like_target_failure(
                &self.history.lock().unwrap().current().clone(),
                &ir_request.safety_constraints,
                &raw,
            );
            ir_request.raw_input = original_raw_input;
            ir_request.confidence = original_confidence;
            push_confirmation_like_fallback_trace(&mut events, &session_context_snapshot);
        } else if ir_request.safety_constraints.no_apply
            && matches!(ir_request.action, IrAction::Apply | IrAction::ReviewSafety)
        {
            if session_context_snapshot.validated_plan.is_some() {
                ir_request.action = IrAction::ReviewValidatedPlan;
                ir_request.mode = ExecutionMode::ValidateOnly;
                if let Some(selected) = session_context_snapshot.selected_candidate.as_ref() {
                    ir_request.target = narrow_target_to_ir_target(&selected.target);
                }
                events.push(CoreEvent::Debug {
                    message:
                        "[IR-TRACE][INTENT_DOWNGRADE] from=Apply to=ReviewValidatedPlan reason=NoApplyWithValidatedPlan"
                            .to_string(),
                });
            } else if session_context_snapshot.selected_candidate.is_some() {
                ir_request.action = IrAction::ValidatePlan;
                ir_request.mode = ExecutionMode::ValidateOnly;
                if let Some(selected) = session_context_snapshot.selected_candidate.as_ref() {
                    ir_request.target = narrow_target_to_ir_target(&selected.target);
                }
                events.push(CoreEvent::Debug {
                    message:
                        "[IR-TRACE][INTENT_DOWNGRADE] from=Apply to=ValidatePlan reason=NoApplyWithSelectedCandidate"
                            .to_string(),
                });
            } else {
                ir_request.action = IrAction::GenerateChangePlan;
                ir_request.mode = ExecutionMode::PlanOnly;
                ir_request.target = session_context_snapshot
                    .previous_analysis_context
                    .as_ref()
                    .map(|ctx| ctx.target.clone())
                    .unwrap_or(IrTarget::WorkspaceRoot);
                events.push(CoreEvent::Debug {
                    message:
                        "[IR-TRACE][INTENT_DOWNGRADE] from=Apply to=GenerateChangePlan reason=NoApplyWithoutSelection"
                            .to_string(),
                });
            }
        }

        // コンテキストアウェアなターゲット解決 (DBM-CONTEXT-AWARE-PLAN-TARGET-RESOLUTION-SPEC v1.0)
        let is_plan_only = is_plan_only_intent(&lower_input);
        let has_context_ref = has_context_reference(&lower_input);

        if is_plan_only && ir_request.action == IrAction::GenerateChangePlan {
            let history = self.history.lock().unwrap();
            let session_context = &history.current().session_context;
            let resolution = resolve_plan_target(
                ir_request.target.clone(),
                is_plan_only,
                has_context_ref,
                Some(session_context),
            );

            if resolution.previous_context_used {
                events.push(CoreEvent::Debug {
                    message: format!(
                        "[IR-TRACE][CONTEXT_RESOLUTION] previous_context_used=true reason={:?} target={} mode={}",
                        resolution.reason, resolution.target, resolution.mode
                    ),
                });
                ir_request.action = resolution.action;
                ir_request.target = resolution.target;
                ir_request.mode = resolution.mode;
            }
        }

        events.push(trace_event(
            "LANGUAGE_CORE",
            json!({
                "intent": ir_request.action.to_string(),
                "confidence": ir_request.confidence,
                "timestamp": timestamp_millis(),
            }),
        ));
        events.push(trace_event(
            "ADAPTER",
            json!({
                "action": ir_request.action.to_string(),
                "target": ir_request.target.to_string(),
                "mode": ir_request.mode.to_string(),
            }),
        ));
        events.push(CoreEvent::Debug {
            message: format!(
                "[IR-TRACE][SAFETY_CONSTRAINTS] no_apply={} no_file_write={} no_git_operation={} no_external_command={}",
                ir_request.safety_constraints.no_apply,
                ir_request.safety_constraints.no_file_write,
                ir_request.safety_constraints.no_git_operation,
                ir_request.safety_constraints.no_external_command
            ),
        });
        if ir_request.action.is_analyze() {
            return self.execute_lc_analyze(ir_request, request, events);
        }
        if ir_request.action == IrAction::ReviewValidatedPlan {
            return self.review_validated_plan(ir_request, events);
        }
        if ir_request.action == IrAction::ReviewSafety {
            return self.review_safety(ir_request, events);
        }
        if ir_request.action == IrAction::ValidatePlan {
            return self.validate_selected_plan(request, events);
        }
        if ir_request.action == IrAction::GenerateChangePlan {
            return self.execute_lc_generate_plan(ir_request, request, events);
        }
        if ir_request.action == IrAction::Apply {
            return self.apply_validated_plan_guard(request, events);
        }
        // ── end LanguageCoreToIrAdapter ──────────────────────────────────────

        let mut intent = Intent::new(request.input.clone());

        // アダプターで解決されたターゲットを Intent に反映 (DBM-CONTEXT-AWARE-PLAN-TARGET-RESOLUTION-SPEC v1.0 §7)
        match &ir_request.target {
            IrTarget::WorkspaceRoot => {
                intent.file = Some(".".to_string());
            }
            IrTarget::File(path) => {
                intent.file = Some(path.clone());
            }
            IrTarget::Symbol(sym) => {
                intent.symbol = Some(sym.clone());
            }
            IrTarget::None => {}
        }

        let ambiguous = requires_clarification(&intent);
        trace_ir!(
            TraceLevel::Basic,
            "CLARIFICATION",
            format!(
                "action={:?}, file={:?}, symbol={:?}, ambiguous={}",
                intent.action, intent.file, intent.symbol, ambiguous
            )
        );
        if ambiguous {
            let candidates = generate_candidates_from_intent_with_limits(&intent, self.limits);
            trace_ir!(TraceLevel::Basic, "PROPOSAL_GENERATED", candidates.len());
            events.push(CoreEvent::Proposal { candidates });
            events.push(CoreEvent::Pipeline {
                state: PipelineState::Proposed.label().to_string(),
            });
            events.push(CoreEvent::Next {
                actions: vec!["select <n> で候補を選択".to_string()],
            });
            return CoreResponse {
                events,
                status: ExecutionStatus::Proposed,
                design: None,
                core_state: None,
            };
        }

        self.execute_coding_pipeline(intent, request, events)
    }

    /// 修正プランの作成要求を直接ハンドルする。
    /// Spec DBM-CONTEXT-AWARE-PLAN-TARGET-RESOLUTION-SPEC v1.0 §11
    fn execute_lc_generate_plan(
        &self,
        ir_request: crate::nl::language_core_ir_adapter::IrIntentRequest,
        request: InternalRequest,
        mut events: Vec<CoreEvent>,
    ) -> CoreResponse {
        let mut intent = Intent::new(request.input.clone());
        match &ir_request.target {
            IrTarget::WorkspaceRoot => intent.file = Some(".".to_string()),
            IrTarget::File(path) => intent.file = Some(path.clone()),
            IrTarget::Symbol(sym) => intent.symbol = Some(sym.clone()),
            IrTarget::None => {}
        }

        let change_candidates = generate_narrow_change_candidates(&request.context.working_dir);
        let candidates = execution_candidates_from_change_candidates(&change_candidates);
        trace_ir!(
            TraceLevel::Basic,
            "PLAN_CANDIDATES",
            format!("count={}", change_candidates.len())
        );
        events.push(CoreEvent::Debug {
            message: format!(
                "[IR-TRACE][PLAN_CANDIDATES] count={} all_narrow=true",
                change_candidates.len()
            ),
        });
        let plan_text = format_change_plan(&ir_request, &change_candidates);
        let plan_hash = stable_context_hash(&plan_text);

        events.push(CoreEvent::Result { message: plan_text });
        events.push(CoreEvent::Proposal {
            candidates: candidates.clone(),
        });
        events.push(CoreEvent::Pipeline {
            state: PipelineState::Proposed.label().to_string(),
        });

        let mut response = CoreResponse {
            events: {
                events.push(CoreEvent::Debug {
                    message: format!(
                        "[IR-TRACE][CONTEXT_STORE] kind=plan target={} mode={} status=Executed",
                        ir_request.target, ir_request.mode
                    ),
                });
                events
            },
            status: ExecutionStatus::Executed,
            design: None,
            core_state: None,
        };

        // CoreState を更新して History に Push する（候補を選択可能にするため）
        let mut history = self.history.lock().unwrap();
        let mut next_state = history.current().clone();
        next_state.proposals = candidates;
        next_state.status = PipelineState::Proposed;
        next_state.preview_snapshot = None;
        next_state.session_context.store_plan(
            ir_request.target.clone(),
            ir_request.mode.clone(),
            next_state.proposals.len(),
            change_candidates,
            plan_hash,
            ExecutionStatus::Executed,
        );
        next_state.previous_analysis_context =
            next_state.session_context.previous_analysis_context.clone();
        history.push(next_state);
        response.core_state = Some(history.current().clone());

        response
    }

    /// LanguageCoreToIrAdapter によって分類された Analyze 系 Intent を直接ハンドルする。
    ///
    /// `execute_from_text` (DefaultIntentRefiner) を経由しないため、
    /// 日本語入力の場合でも InvalidInput にならない。
    fn execute_lc_analyze(
        &self,
        ir_request: crate::nl::language_core_ir_adapter::IrIntentRequest,
        request: InternalRequest,
        mut events: Vec<CoreEvent>,
    ) -> CoreResponse {
        let id = request.id;
        let working_dir = request.context.working_dir.clone();
        let path_str = working_dir.to_str().unwrap_or(".");

        if observability_enabled() {
            println!(
                "[LC-ANALYZE][id={}] action={} target={} mode={}",
                id, ir_request.action, ir_request.target, ir_request.mode
            );
        }

        events.push(CoreEvent::Thinking {
            summary: format!(
                "analyzing intent via capability registry / IntentをCapability Registry経由で解析中 [{}]",
                ir_request.action
            ),
        });

        // DBM-RUNTIME-DISPATCH-INTEGRATION-SPEC v1.0 §8
        // RuntimeAnalyzeDispatcher を使用して Capability をディスパッチ
        let analysis_text = match RuntimeAnalyzeDispatcher::dispatch(&ir_request.action, path_str) {
            Ok((result, output_type, _capability)) => {
                format_capability_result(result.as_ref(), output_type, path_str)
            }
            Err(err) => {
                trace_ir!(
                    TraceLevel::Error,
                    "CAPABILITY_ERROR",
                    format!(
                        "reason=DispatchFailed action={} error={err}",
                        ir_request.action
                    )
                );
                format!(
                    "# Analysis Failed\n\nAction: {}\n\nError: {}\n\nHint: The intent may not be supported or a capability mismatch occurred.",
                    ir_request.action, err
                )
            }
        };

        events.push(CoreEvent::Result {
            message: analysis_text,
        });
        events.push(CoreEvent::Debug {
            message: format!(
                "[IR-TRACE][CONTEXT_STORE] kind=analysis action={} target={} mode={} status=Executed",
                ir_request.action, ir_request.target, ir_request.mode
            ),
        });
        events.push(CoreEvent::Pipeline {
            state: PipelineState::Planned.label().to_string(),
        });

        let mut response = CoreResponse {
            events,
            status: ExecutionStatus::Executed,
            design: None,
            core_state: None,
        };

        // 前回の解析コンテキストを保存
        let mut history = self.history.lock().unwrap();
        let mut next_state = history.current().clone();
        next_state.preview_snapshot = None;
        next_state.session_context.store_analysis(
            ir_request.action.clone(),
            ir_request.target.clone(),
            ir_request.mode.clone(),
            ExecutionStatus::Executed,
        );
        next_state.previous_analysis_context =
            next_state.session_context.previous_analysis_context.clone();
        history.push(next_state);
        response.core_state = Some(history.current().clone());

        response
    }

    fn review_validated_plan(
        &self,
        ir_request: crate::nl::language_core_ir_adapter::IrIntentRequest,
        mut events: Vec<CoreEvent>,
    ) -> CoreResponse {
        let current = self.history.lock().unwrap().current().clone();
        let Some(validated) = current.session_context.validated_plan.as_ref() else {
            events.push(CoreEvent::Debug {
                message:
                    "[IR-TRACE][INTENT_DOWNGRADE] from=ReviewValidatedPlan to=ValidatePlan reason=MissingValidatedPlan"
                        .to_string(),
            });
            return CoreResponse {
                events,
                status: ExecutionStatus::Failed,
                design: None,
                core_state: Some(current),
            };
        };

        events.push(CoreEvent::Debug {
            message: format!(
                "[IR-TRACE][VALIDATED_PLAN_PRESERVE] candidate_id={} target={}",
                validated.candidate_id, validated.target
            ),
        });
        events.push(CoreEvent::Debug {
            message: "[IR-TRACE][APPLY_DEFERRED] reason=NoApplyConstraint".to_string(),
        });
        events.push(CoreEvent::Result {
            message: format_validation_review(
                validated,
                current.session_context.previous_validation_context.as_ref(),
                ir_request.safety_constraints.no_apply,
                matches!(
                    ir_request.target_failure,
                    Some(
                        crate::nl::normalization::TargetResolutionFailure::ConfirmationTokenLike {
                            ..
                        }
                    )
                ),
            ),
        });

        CoreResponse {
            events,
            status: ExecutionStatus::Executed,
            design: None,
            core_state: Some(current),
        }
    }

    fn review_safety(
        &self,
        ir_request: crate::nl::language_core_ir_adapter::IrIntentRequest,
        mut events: Vec<CoreEvent>,
    ) -> CoreResponse {
        if !matches!(
            ir_request.target_failure,
            Some(crate::nl::normalization::TargetResolutionFailure::ConfirmationTokenLike { .. })
        ) {
            events.push(CoreEvent::Debug {
                message: "[IR-TRACE][INTENT_DOWNGRADE] to=ReviewSafety reason=SafetyReviewReadOnly"
                    .to_string(),
            });
        }
        events.push(CoreEvent::Debug {
            message: "[IR-TRACE][APPLY_DEFERRED] reason=ReviewSafetyReadOnly".to_string(),
        });
        events.push(CoreEvent::Result {
            message: format_safety_review(
                ir_request.safety_constraints.no_apply,
                matches!(
                    ir_request.target_failure,
                    Some(
                        crate::nl::normalization::TargetResolutionFailure::ConfirmationTokenLike {
                            ..
                        }
                    )
                ),
            ),
        });

        CoreResponse {
            events,
            status: ExecutionStatus::Executed,
            design: None,
            core_state: Some(self.history.lock().unwrap().current().clone()),
        }
    }

    fn validate_selected_plan(
        &self,
        request: InternalRequest,
        mut events: Vec<CoreEvent>,
    ) -> CoreResponse {
        let mut history = self.history.lock().unwrap();
        let current = history.current().clone();
        let Some(plan_ctx) = current.session_context.previous_plan_context.as_ref() else {
            return error_response(
                "ClarificationRequired",
                "no previous plan to validate",
                request.id,
            );
        };
        let Some(selected) = current.session_context.selected_candidate.as_ref() else {
            return error_response(
                "ClarificationRequired",
                "select a candidate before validation",
                request.id,
            );
        };
        let Some(candidate) = plan_ctx
            .candidates
            .iter()
            .find(|candidate| candidate.candidate_id == selected.candidate_id)
            .cloned()
        else {
            return error_response(
                "ValidationError",
                "selected candidate not found",
                request.id,
            );
        };

        let validation = validate_change_plan_candidate(
            &request.context.working_dir,
            plan_ctx.plan_hash,
            &candidate,
            selected,
        );
        trace_ir!(
            TraceLevel::Basic,
            "PLAN_VALIDATION",
            format!(
                "candidate_id={} status={:?} apply_allowed={}",
                validation.candidate_id, validation.status, validation.apply_allowed
            )
        );
        events.push(CoreEvent::Debug {
            message: format!(
                "[IR-TRACE][PLAN_VALIDATION] candidate_id={} status={:?} apply_allowed={}",
                validation.candidate_id, validation.status, validation.apply_allowed
            ),
        });
        events.push(CoreEvent::Result {
            message: format_plan_validation(&validation),
        });

        let mut next_state = current;
        next_state
            .session_context
            .store_validation(validation.clone());
        if validation.apply_allowed {
            events.push(CoreEvent::Debug {
                message: "[IR-TRACE][CONTEXT_STORE] kind=validated_plan apply_allowed=true"
                    .to_string(),
            });
        }
        next_state.previous_analysis_context =
            next_state.session_context.previous_analysis_context.clone();
        history.push(next_state);
        let core_state = history.current().clone();
        drop(history);

        CoreResponse {
            events,
            status: ExecutionStatus::Executed,
            design: None,
            core_state: Some(core_state),
        }
    }

    fn apply_validated_plan_guard(
        &self,
        request: InternalRequest,
        mut events: Vec<CoreEvent>,
    ) -> CoreResponse {
        let current = self.history.lock().unwrap().current().clone();
        let decision = apply_guard_decide(ApplyGuardInput::from(&current));
        if let ApplyGuardDecision::Reject { reason } = decision {
            trace_ir!(
                TraceLevel::Basic,
                "APPLY_GUARD",
                format!("rejected=true reason={reason}")
            );
            let next_state = self.clear_preview_state(PipelineState::Idle);
            events.push(CoreEvent::Debug {
                message: format!("[IR-TRACE][APPLY_GUARD] rejected=true reason={reason}"),
            });
            events.push(CoreEvent::Debug {
                message:
                    "[IR-TRACE][CONTEXT_CLEAR] reason=ApplyGuardReject clears preview/selection"
                        .to_string(),
            });
            events.push(CoreEvent::Debug {
                message: "[IR-TRACE][PIPELINE_DERIVE] state=Idle reason=ApplyRejected".to_string(),
            });
            events.push(CoreEvent::Result {
                message: format!("# Apply Rejected\n\nReason: {reason}.\n\nNo files modified."),
            });
            events.push(CoreEvent::Pipeline {
                state: PipelineState::Idle.label().to_string(),
            });
            return CoreResponse {
                events,
                status: ExecutionStatus::Failed,
                design: None,
                core_state: Some(next_state),
            };
        }

        let ApplyGuardDecision::Allow {
            candidate_id,
            ref target,
            validated_plan_hash,
        } = decision
        else {
            unreachable!("reject returned above")
        };
        let allow_trace = format!(
            "rejected=false candidate_id={candidate_id} target={target} validated_plan_hash={validated_plan_hash}"
        );
        trace_ir!(TraceLevel::Basic, "APPLY_GUARD", allow_trace.clone());
        events.push(CoreEvent::Debug {
            message: format!("[IR-TRACE][APPLY_GUARD] {allow_trace}"),
        });

        let apply_result = self.execute_safe_apply(
            &request.context.working_dir,
            &current,
            &decision,
            &mut events,
        );
        match apply_result {
            ApplyResult::Applied {
                transaction_id,
                post_apply_checksum,
            } => {
                self.pending_files.lock().expect("pending lock").clear();
                let next_state = self.clear_preview_state(PipelineState::Idle);
                events.push(CoreEvent::Result {
                    message: format!(
                        "# Safe Apply\n\nTransaction: {transaction_id}\nCandidate: {candidate_id}\nTarget: {target}\nPost checksum: {post_apply_checksum}\n\nApplied."
                    ),
                });
                events.push(CoreEvent::Pipeline {
                    state: PipelineState::Idle.label().to_string(),
                });
                CoreResponse {
                    events,
                    status: ExecutionStatus::Executed,
                    design: None,
                    core_state: Some(next_state),
                }
            }
            ApplyResult::RolledBack {
                transaction_id,
                reason,
            } => {
                let next_state = self.preserve_preview_state(PipelineState::Previewed);
                events.push(CoreEvent::Result {
                    message: format!(
                        "# Safe Apply Rolled Back\n\nTransaction: {transaction_id}\nReason: {reason}.\n\nPreview context preserved."
                    ),
                });
                events.push(CoreEvent::Pipeline {
                    state: PipelineState::Previewed.label().to_string(),
                });
                CoreResponse {
                    events,
                    status: ExecutionStatus::Failed,
                    design: None,
                    core_state: Some(next_state),
                }
            }
            ApplyResult::Rejected { reason } => {
                events.push(CoreEvent::Result {
                    message: format!(
                        "# Safe Apply Rejected\n\nReason: {reason}.\n\nNo files modified."
                    ),
                });
                CoreResponse {
                    events,
                    status: ExecutionStatus::Failed,
                    design: None,
                    core_state: Some(current),
                }
            }
        }
    }

    fn execute_coding_pipeline(
        &self,
        intent: Intent,
        request: InternalRequest,
        mut events: Vec<CoreEvent>,
    ) -> CoreResponse {
        let id = request.id;
        let chat_context = runtime_core::ChatContext::default();
        let runtime_result = match self
            .runtime
            .execute_from_text(&request.input, &chat_context)
        {
            Ok(RuntimeExecutionResult::Executed(result)) => result,
            Ok(RuntimeExecutionResult::Clarification(clarification)) => {
                if let Some(target) = log_clear_intent_runtime_clarification_bypassed(
                    &mut events,
                    &intent,
                    &request.input,
                    &clarification,
                ) {
                    return self
                        .execute_clarification_bypassed_pipeline(intent, request, events, target);
                }

                let trace = ir_trace_json(
                    "ERROR",
                    json!({
                        "status": "clarification_required",
                        "error": clarification.message,
                        "timestamp": timestamp_millis(),
                    }),
                );
                trace_ir!(TraceLevel::Error, "ERROR", trace);
                append_error_with_recovery(
                    &mut events,
                    &format!("clarification required: {}", clarification.message),
                );
                return error_response("ClarificationRequired", &clarification.message, id);
            }
            Err(err) => {
                let trace = ir_trace_json(
                    "ERROR",
                    json!({
                        "status": "runtime_error",
                        "error": format!("{err:?}"),
                        "timestamp": timestamp_millis(),
                    }),
                );
                trace_ir!(TraceLevel::Error, "ERROR", trace);
                append_error_with_recovery(&mut events, &format!("core execution failed: {err:?}"));
                return error_response("RuntimeError", &format!("{err:?}"), id);
            }
        };

        if observability_enabled() {
            println!("[CODING][id={}] stage=plan", id);
        }

        events.push(CoreEvent::Thinking {
            summary: "strategy execution started / 戦略実行を開始しました".to_string(),
        });
        events.push(trace_event(
            "IR",
            ir_plan_json(&runtime_result.execution_plan),
        ));
        events.push(trace_event(
            "EXEC_PLAN",
            execution_plan_json(&runtime_result.execution_plan),
        ));

        {
            let ir = &runtime_result.execution_plan;
            let ir_steps = ir.dependency_plan.install_commands.len()
                + ir.build_plan.build_commands.len()
                + ir.run_plan.run_commands.len()
                + ir.test_plan.test_commands.len();
            trace_ir!(TraceLevel::Basic, "COUNT", format!("IR_STEPS={ir_steps}"));
        }

        let strategy_input = StrategyInput {
            intent,
            initial_plan: runtime_result.execution_plan.clone(),
            context: StrategyExecutionContext {
                repo_root: request.context.working_dir.clone(),
                ..StrategyExecutionContext::default()
            },
            history: ExecutionHistory::new(),
        };
        let runner = DryRunIntegrator;
        let strategy_output = self.strategy.execute(strategy_input, &runner);
        events.push(trace_event(
            "CANDIDATES",
            candidates_json_from_strategy(&strategy_output),
        ));
        events.push(trace_event(
            "SELECTED",
            selected_json_from_strategy(&strategy_output),
        ));
        events.push(trace_event(
            "EXECUTION",
            execution_result_json(&strategy_output),
        ));
        events.push(CoreEvent::Execution {
            step: "strategy execution completed".to_string(),
        });
        events.extend(core_events_from_strategy(&strategy_output));

        if !strategy_output.success {
            let step = selected_json_from_strategy(&strategy_output);
            let trace = ir_trace_json(
                "ERROR",
                json!({
                    "error": strategy_output.strategy_trace.final_outcome.to_string(),
                    "selected": step,
                    "timestamp": timestamp_millis(),
                }),
            );
            trace_ir!(TraceLevel::Error, "ERROR", trace);
            append_error_with_recovery(
                &mut events,
                &strategy_output.strategy_trace.final_outcome.to_string(),
            );
            return error_response(
                "StrategyError",
                &strategy_output.strategy_trace.final_outcome.to_string(),
                id,
            );
        }

        self.store_pending_files(&runtime_result);

        if observability_enabled() {
            println!("[CODING][id={}] stage=preview", id);
        }

        events.push(CoreEvent::Result {
            message: "core execution completed".to_string(),
        });
        events.push(CoreEvent::Pipeline {
            state: PipelineState::Planned.label().to_string(),
        });
        let pending = self.pending_files.lock().expect("pending lock").clone();
        events.push(CoreEvent::Preview {
            diff: if pending.is_empty() {
                vec!["(no pending files)".to_string()]
            } else {
                preview_lines(&pending)
            },
        });
        events.extend(diff_events_from_pending(&pending));
        events.push(CoreEvent::Pipeline {
            state: PipelineState::Previewed.label().to_string(),
        });

        if observability_enabled() {
            println!("[CODING][id={}] stage=apply", id);
        }

        // §6 IF ambiguous=false → execute_pipeline (automatic apply)
        let mut apply_request = request.clone();
        apply_request.context.pipeline_state = PipelineState::Previewed;
        let apply_response = self.apply(&apply_request);
        events.extend(apply_response.events);

        let design = design_document_from_core_result(
            &runtime_result,
            &strategy_output,
            request.context.design_snapshot.as_ref(),
        );
        events.push(CoreEvent::DesignUpdate {
            summary: design_summary(&design),
            score: design_score(&design),
        });
        if let Some(previous) = request.context.design_snapshot.as_ref() {
            events.push(CoreEvent::DesignDiff {
                changes: design_diff(previous, &design),
            });
        }
        CoreResponse {
            events,
            status: ExecutionStatus::Executed,
            design: Some(design),
            core_state: None,
        }
    }

    fn execute_clarification_bypassed_pipeline(
        &self,
        intent: Intent,
        request: InternalRequest,
        mut events: Vec<CoreEvent>,
        target: String,
    ) -> CoreResponse {
        let id = request.id;
        if observability_enabled() {
            println!("[CODING][id={}] stage=plan", id);
        }

        let action = format!("{:?}", intent.action).to_ascii_lowercase();
        events.push(CoreEvent::Plan {
            steps: vec![format!("{action} {target}")],
        });
        events.push(CoreEvent::Pipeline {
            state: PipelineState::Planned.label().to_string(),
        });

        let (resolved_target, target_path) =
            match resolve_clarification_target(&request.context.working_dir, &target) {
                Ok(resolved) => resolved,
                Err(err) => return error_response("SafetyViolation", &err, id),
            };
        let original = match fs::read_to_string(&target_path) {
            Ok(content) => content,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => {
                return error_response(
                    "ExecutionError",
                    &format!("cannot read clarification target: {err}"),
                    id,
                );
            }
        };
        let updated = append_comment_line(&original, &resolved_target);
        let pending_file = PendingFile {
            path: resolved_target.clone(),
            content_checksum: checksum_bytes(updated.as_bytes()),
            content: updated,
        };
        *self.pending_files.lock().expect("pending lock") = vec![pending_file.clone()];
        self.applied_files.lock().expect("applied lock").clear();

        if observability_enabled() {
            println!("[CODING][id={}] stage=preview", id);
        }
        events.push(CoreEvent::Preview {
            diff: preview_lines(std::slice::from_ref(&pending_file)),
        });
        events.extend(diff_events_from_pending(std::slice::from_ref(
            &pending_file,
        )));
        events.push(CoreEvent::Pipeline {
            state: PipelineState::Previewed.label().to_string(),
        });

        if observability_enabled() {
            println!("[CODING][id={}] stage=apply", id);
        }
        let mut apply_request = request.clone();
        apply_request.input = format!("refactor {resolved_target}");
        apply_request.context.pipeline_state = PipelineState::Previewed;
        let apply_response = self.apply(&apply_request);
        let status = apply_response.status;
        events.extend(apply_response.events);
        if status == ExecutionStatus::Failed {
            return CoreResponse {
                events,
                status,
                design: None,
                core_state: None,
            };
        }

        events.push(CoreEvent::Execution {
            step: format!("execute {action} on {resolved_target}"),
        });
        events.push(CoreEvent::Result {
            message: "core execution completed".to_string(),
        });
        let design = DesignDocument::new(
            request
                .context
                .design_snapshot
                .as_ref()
                .map(|doc| doc.version.saturating_add(1))
                .unwrap_or(1),
            vec![ReasonUnit {
                id: "clarification-bypassed".to_string(),
                title: "clarification bypassed".to_string(),
                summary: format!("clear intent executed for {resolved_target}"),
            }],
            StructureTree {
                module: request.context.working_dir.display().to_string(),
                functions: vec![resolved_target],
            },
            vec![Constraint {
                text: "Clarification bypass must continue to execution".to_string(),
            }],
        );
        CoreResponse {
            events,
            status: ExecutionStatus::Executed,
            design: Some(design),
            core_state: None,
        }
    }

    /// Derive a new `CoreState` from `response`, deduplicate via the state
    /// graph, push to `history`, and attach.  Spec DBM-GRAPH-INTEGRATION-STEP2 §7.
    ///
    /// Flow:
    /// 1. Derive `next` from events.
    /// 2. Compute hashes of `prev` (current) and `next` (new).
    /// 3. Graph lookup: reuse existing node or insert `next`.
    /// 4. On cycle (new hash == current hash), push `next` as-is without graph insertion.
    /// 5. Push canonical state to History and attach to response.
    fn push_and_attach_core_state(&self, mut response: CoreResponse, id: u64) -> CoreResponse {
        let mut hist = self.history.lock().expect("history lock");
        let mut graph = self.state_graph.lock().expect("state_graph lock");

        let prev = hist.current().clone();
        let current_hash = StateGraph::state_hash(&prev);
        let mut next = core_state_from_events(&prev, &response.events, response.design.as_ref());
        if is_exploration_state(&next.status)
            && StateGraph::state_hash(&next) != StateGraph::state_hash(&prev)
        {
            next.depth = prev.depth.saturating_add(1).min(self.limits.max_depth);
        }

        // Graph integration: reuse or insert (spec §6-§8)
        let canonical = match graph.reuse_or_insert(next.clone(), current_hash, id) {
            Ok(state) => state,
            Err(_) => next, // Cycle detected — push next as-is (no graph node added)
        };

        hist.push(canonical.clone());
        response.core_state = Some(canonical);
        response
    }

    fn current_depth(&self) -> usize {
        self.history.lock().expect("history lock").current().depth
    }

    fn max_depth_response(&self) -> CoreResponse {
        let current = self.history.lock().expect("history lock").current().clone();
        CoreResponse {
            events: vec![
                CoreEvent::Result {
                    message: "Max depth reached".to_string(),
                },
                CoreEvent::Next {
                    actions: vec!["undo".to_string(), "jump <version>".to_string()],
                },
            ],
            status: ExecutionStatus::Idle,
            design: current.design.clone(),
            core_state: Some(current),
        }
    }

    fn execute_command(&self, request: &InternalRequest) -> CoreResponse {
        if let Some(response) = self.execute_pipeline_builtin(request) {
            return response;
        }

        let id = request.id;
        let input = request.input.trim();
        let parts: Vec<&str> = input.split_whitespace().collect();
        if parts.is_empty() {
            return error_response("CommandError", "Empty command", id);
        }

        let command = parts[0];
        let args = &parts[1..];
        self.execute_adapter_command(command, args, request)
    }

    fn execute_pipeline_builtin(&self, request: &InternalRequest) -> Option<CoreResponse> {
        let id = request.id;
        let input = request.input.trim();

        let lower = input.to_ascii_lowercase();
        match lower.as_str() {
            "proposals" | "reselect" => Some(self.show_proposals(request)),
            "compare" => Some(self.compare_proposals(request)),
            "apply" | "y" | "yes" => Some(self.apply(request)),
            "preview" => Some(self.preview(request)),
            "cancel" | "n" | "no" => Some(self.cancel(request)),
            "undo" => Some(self.undo(request)),
            "commit" | "commit changes" => Some(self.git_commit(request)),
            "rollback" => Some(self.rollback(request)),
            "replay" => Some(self.replay(request, None)),
            _ if lower.starts_with("replay ") => {
                Some(self.replay(request, input.split_whitespace().nth(1)))
            }
            _ if lower.starts_with("filter ") => Some(self.filter(input, id)),
            _ if lower.starts_with("git add ") => Some(self.git_add(request, input)),
            _ if lower.starts_with("select ") => Some(self.select_candidate(request, input)),
            _ if lower.starts_with("jump ") => Some(self.jump(request, input)),
            _ if is_forbidden_command(&lower) => Some(error_response("SafetyViolation", input, id)),
            _ => None,
        }
    }

    fn route_preview_input(&self, request: InternalRequest) -> CoreResponse {
        let action = parse_preview_confirmation(&request.input);
        let mut events = vec![
            CoreEvent::Debug {
                message: format!("[IR-TRACE][PREVIEW_CONFIRMATION] action={action:?}"),
            },
            CoreEvent::Debug {
                message: confirmation_token_check_trace(&request.input, action),
            },
        ];

        match action {
            PreviewConfirmation::Confirm => self.apply_validated_plan_guard(request, events),
            PreviewConfirmation::Reject => self.reject_preview(events),
            PreviewConfirmation::Cancel => self.cancel_preview(events),
            PreviewConfirmation::Reconfirm => {
                let current = self.history.lock().expect("history lock").current().clone();
                if !preview_state_is_valid(
                    &current.session_context,
                    current.preview_snapshot.as_ref(),
                ) {
                    let mut downgraded = current;
                    downgraded.status = derive_pipeline_state_from_context(
                        &downgraded.session_context,
                        downgraded.preview_snapshot.as_ref(),
                    );
                    let derived = downgraded.status.clone();
                    self.history
                        .lock()
                        .expect("history lock")
                        .replace_current(downgraded.clone());
                    events.extend([
                        CoreEvent::Debug {
                            message: format!(
                                "[IR-TRACE][ROLLBACK_STATE_CHECK] restored_state=Previewed valid=false reason={}",
                                preview_invalid_reason(&downgraded)
                            ),
                        },
                        CoreEvent::Debug {
                            message: format!(
                                "[IR-TRACE][PIPELINE_DERIVE] state={} reason={}",
                                derived.label(),
                                pipeline_derive_reason(
                                    &downgraded.session_context,
                                    downgraded.preview_snapshot.as_ref()
                                )
                            ),
                        },
                        CoreEvent::Pipeline {
                            state: derived.label().to_string(),
                        },
                        CoreEvent::Next {
                            actions: next_actions_for_pipeline_state(&derived),
                        },
                    ]);
                    return CoreResponse {
                        events,
                        status: ExecutionStatus::Idle,
                        design: None,
                        core_state: Some(downgraded),
                    };
                }
                if should_route_preview_input_to_language(&request.input) {
                    let mut response = self.execute_natural_language(request);
                    events.extend(response.events);
                    response.events = events;
                    return response;
                }
                events.extend([
                    CoreEvent::Result {
                        message: "Please confirm: y / n / cancel".to_string(),
                    },
                    CoreEvent::Pipeline {
                        state: PipelineState::Previewed.label().to_string(),
                    },
                    CoreEvent::Next {
                        actions: vec!["y".to_string(), "n".to_string(), "cancel".to_string()],
                    },
                ]);
                CoreResponse {
                    events,
                    status: ExecutionStatus::Idle,
                    design: None,
                    core_state: Some(current),
                }
            }
        }
    }

    fn reject_preview(&self, mut events: Vec<CoreEvent>) -> CoreResponse {
        self.pending_files.lock().expect("pending lock").clear();
        let next_state = self.clear_preview_state(PipelineState::Idle);
        events.extend([
            CoreEvent::Debug {
                message: "[IR-TRACE][CONTEXT_CLEAR] reason=PreviewReject clears preview/selection/validation"
                    .to_string(),
            },
            CoreEvent::Debug {
                message: "[IR-TRACE][PIPELINE_DERIVE] state=Idle reason=NoSelectedCandidateNoPreview"
                    .to_string(),
            },
            CoreEvent::Result {
                message: "Preview cancelled. No files modified.".to_string(),
            },
            CoreEvent::Pipeline {
                state: PipelineState::Idle.label().to_string(),
            },
            CoreEvent::Next { actions: vec![] },
        ]);
        CoreResponse {
            events,
            status: ExecutionStatus::Idle,
            design: None,
            core_state: Some(next_state),
        }
    }

    fn cancel_preview(&self, mut events: Vec<CoreEvent>) -> CoreResponse {
        self.pending_files.lock().expect("pending lock").clear();
        let next_state = self.clear_preview_state(PipelineState::Idle);
        events.extend([
            CoreEvent::Debug {
                message: "[IR-TRACE][CONTEXT_CLEAR] reason=PreviewCancel clears preview/selection/validation"
                    .to_string(),
            },
            CoreEvent::Debug {
                message: "[IR-TRACE][PIPELINE_DERIVE] state=Idle reason=NoSelectedCandidateNoPreview"
                    .to_string(),
            },
            CoreEvent::Result {
                message: "Preview cancelled. No files modified.".to_string(),
            },
            CoreEvent::Pipeline {
                state: PipelineState::Idle.label().to_string(),
            },
            CoreEvent::Next { actions: vec![] },
        ]);
        CoreResponse {
            events,
            status: ExecutionStatus::Idle,
            design: None,
            core_state: Some(next_state),
        }
    }

    fn clear_preview_state(&self, status: PipelineState) -> CoreState {
        let mut history = self.history.lock().expect("history lock");
        let mut next_state = history.current().clone();
        next_state.status = status;
        next_state.preview_snapshot = None;
        next_state.session_context.selected_candidate = None;
        next_state.session_context.validated_plan = None;
        next_state.session_context.previous_validation_context = None;
        next_state.previous_analysis_context =
            next_state.session_context.previous_analysis_context.clone();
        history.push(next_state.clone());
        next_state
    }

    fn preserve_preview_state(&self, status: PipelineState) -> CoreState {
        let mut history = self.history.lock().expect("history lock");
        let mut next_state = history.current().clone();
        next_state.status = status;
        next_state.previous_analysis_context =
            next_state.session_context.previous_analysis_context.clone();
        history.push(next_state.clone());
        next_state
    }

    fn execute_safe_apply(
        &self,
        root: &Path,
        state: &CoreState,
        guard_decision: &ApplyGuardDecision,
        events: &mut Vec<CoreEvent>,
    ) -> ApplyResult {
        let ApplyGuardDecision::Allow {
            candidate_id,
            target,
            validated_plan_hash,
        } = guard_decision
        else {
            return ApplyResult::Rejected {
                reason: ApplyRejectReason::GuardRejected,
            };
        };
        let Some(validated) = state.session_context.validated_plan.as_ref() else {
            return ApplyResult::Rejected {
                reason: ApplyRejectReason::MissingValidatedPlan,
            };
        };
        if state.session_context.selected_candidate.is_none() {
            return ApplyResult::Rejected {
                reason: ApplyRejectReason::MissingSelectedCandidate,
            };
        }
        let Target::File(relative_target) = target else {
            let reason = if *target == Target::WorkspaceRoot {
                ApplyRejectReason::WorkspaceRootApplyForbidden
            } else {
                ApplyRejectReason::UnsupportedTarget
            };
            return ApplyResult::Rejected { reason };
        };

        let pending = self.pending_files.lock().expect("pending lock").clone();
        if pending.len() != 1 || pending[0].path != *relative_target {
            return ApplyResult::Rejected {
                reason: ApplyRejectReason::MissingPreview,
            };
        }
        let Some(preview) = state.preview_snapshot.as_ref() else {
            return ApplyResult::Rejected {
                reason: ApplyRejectReason::MissingPreview,
            };
        };
        let planned_diff_checksum = checksum_u64(preview_lines(&pending).join("\n").as_bytes());
        if planned_diff_checksum != preview.preview_hash {
            return ApplyResult::Rejected {
                reason: ApplyRejectReason::ChecksumMismatch,
            };
        }

        let target_path = match resolve_repo_file(root, relative_target) {
            Ok(path) => path,
            Err(_) => {
                return ApplyResult::Rejected {
                    reason: ApplyRejectReason::UnsupportedTarget,
                };
            }
        };
        let original_contents = match fs::read_to_string(&target_path) {
            Ok(contents) => contents,
            Err(_) => {
                return ApplyResult::RolledBack {
                    transaction_id: 0,
                    reason: ApplyFailureReason::TargetMissing,
                };
            }
        };
        let transaction = create_safe_apply_transaction(
            *candidate_id,
            *validated_plan_hash,
            target.clone(),
            target_path.clone(),
            original_contents,
            planned_diff_checksum,
        );
        events.extend([
            CoreEvent::Debug {
                message: format!(
                    "[IR-TRACE][SAFE_APPLY_BEGIN] transaction_id={} candidate_id={} target={}",
                    transaction.transaction_id, transaction.candidate_id, transaction.target
                ),
            },
            CoreEvent::Debug {
                message: format!(
                    "[IR-TRACE][ROLLBACK_SNAPSHOT_CREATED] checksum={}",
                    transaction.rollback_snapshot.original_checksum
                ),
            },
        ]);

        if checksum_u64(transaction.rollback_snapshot.original_contents.as_bytes())
            != transaction.rollback_snapshot.original_checksum
        {
            return ApplyResult::Rejected {
                reason: ApplyRejectReason::ChecksumMismatch,
            };
        }
        if validated.plan_hash != *validated_plan_hash {
            return rollback_safe_apply(&transaction, ApplyFailureReason::ChecksumMismatch, events);
        }
        if fs::write(&target_path, &pending[0].content).is_err() {
            return rollback_safe_apply(&transaction, ApplyFailureReason::DiffApplyFailed, events);
        }
        let post_apply_contents = match fs::read_to_string(&target_path) {
            Ok(contents) => contents,
            Err(_) => {
                return rollback_safe_apply(
                    &transaction,
                    ApplyFailureReason::PostValidationFailed,
                    events,
                );
            }
        };
        let post_apply_checksum = checksum_u64(post_apply_contents.as_bytes());
        if post_apply_checksum != checksum_u64(pending[0].content.as_bytes()) {
            return rollback_safe_apply(&transaction, ApplyFailureReason::ChecksumMismatch, events);
        }
        if post_apply_checksum == transaction.pre_apply_checksum
            || !post_apply_validation(&target_path, &post_apply_contents)
        {
            return rollback_safe_apply(
                &transaction,
                ApplyFailureReason::PostValidationFailed,
                events,
            );
        }
        events.push(CoreEvent::Debug {
            message: format!(
                "[IR-TRACE][SAFE_APPLY_SUCCESS] transaction_id={} post_checksum={}",
                transaction.transaction_id, post_apply_checksum
            ),
        });
        ApplyResult::Applied {
            transaction_id: transaction.transaction_id,
            post_apply_checksum,
        }
    }

    fn execute_git_command(
        &self,
        command: GitCommand,
        id: u64,
        working_dir: &Path,
    ) -> CoreResponse {
        let policy = git_command_policy(&command);
        if matches!(
            policy.command_type,
            CommandType::Dangerous | CommandType::Forbidden
        ) {
            return error_response("ExecutionRejected", &command.canonical(), id);
        }

        let result = match &command {
            GitCommand::Status | GitCommand::Diff | GitCommand::Log => {
                git_execute_read(working_dir, &command)
            }
            GitCommand::AddFile(path) => self.execute_scoped_git_add(working_dir, path),
            GitCommand::Commit { .. } => self.execute_fixed_git_commit(working_dir),
        };

        let (status, message, result_label) = match result {
            Ok(output) => (
                ExecutionStatus::Executed,
                normalize_git_output(&output),
                "success".to_string(),
            ),
            Err(err) => (
                ExecutionStatus::Failed,
                err.clone(),
                format!("failed:{err}"),
            ),
        };
        let record = GitExecutionRecord {
            command: command.canonical(),
            targets: command.targets(),
            result: result_label,
            timestamp: timestamp_millis() as u64,
        };
        let telemetry = git_record_json(&record);

        CoreResponse {
            events: vec![
                CoreEvent::Execution {
                    step: command.canonical(),
                },
                CoreEvent::Debug { message: telemetry },
                if status == ExecutionStatus::Executed {
                    CoreEvent::Result { message }
                } else {
                    CoreEvent::Error {
                        message: format!("ExecutionError: {message}"),
                    }
                },
            ],
            status,
            design: None,
            core_state: None,
        }
    }

    fn execute_scoped_git_add(&self, working_dir: &Path, path: &Path) -> Result<String, String> {
        validate_git_add_path(&path.to_string_lossy())?;
        reject_dirty_worktree_except(working_dir, &DirtyTreePolicy::default(), Some(path))?;
        git_add_file(working_dir, path)?;
        Ok(format!("Staged: {}", path.display()))
    }

    fn execute_fixed_git_commit(&self, working_dir: &Path) -> Result<String, String> {
        reject_unstaged_worktree(working_dir, &DirtyTreePolicy::default())?;
        if git_lines(working_dir, &["diff", "--cached", "--name-only"])?.is_empty() {
            return Err("git commit requires staged changes".to_string());
        }
        let hash = git_commit_fixed(working_dir)?;
        Ok(format!("Committed: {hash}"))
    }

    fn execute_adapter_command(
        &self,
        command: &str,
        args: &[&str],
        request: &InternalRequest,
    ) -> CoreResponse {
        let id = request.id;
        if observability_enabled() {
            println!(
                "[EXEC][id={}] adapter={} target={:?}",
                id,
                command,
                args.first()
            );
            println!("[EXEC_STEP][id={}] resolve_target", id);
        }
        let mut session = crate::session::AgentSession::new();
        let subcommand = args.first().cloned();
        if observability_enabled() {
            println!("[EXEC_STEP][id={}] build_plan", id);
        }
        let remaining_args = if args.len() > 1 {
            args[1..].iter().map(|s| s.to_string()).collect()
        } else {
            Vec::new()
        };

        if observability_enabled() {
            println!("[EXEC_STEP][id={}] validate", id);
        }
        let response =
            match self
                .registry
                .execute(command, subcommand, &remaining_args, &mut session)
            {
                Ok(output) => {
                    if observability_enabled() {
                        println!("[EXEC_STEP][id={}] apply_patch", id);
                    }
                    CoreResponse {
                        events: vec![CoreEvent::Result {
                            message: output.message,
                        }],
                        status: ExecutionStatus::Executed,
                        design: None,
                        core_state: None,
                    }
                }
                Err(err) => error_response("CommandError", &err.to_string(), id),
            };

        if observability_enabled() {
            println!(
                "[EXEC][id={}] adapter={} status={}",
                id,
                command,
                execution_status_label(response.status)
            );
        }
        response
    }

    fn show_proposals(&self, request: &InternalRequest) -> CoreResponse {
        let id = request.id;
        if self.current_depth() >= self.limits.max_depth {
            let current = self.history.lock().expect("history lock").current().clone();
            return CoreResponse {
                events: vec![CoreEvent::Error {
                    message: "Reselect disabled at max depth".to_string(),
                }],
                status: ExecutionStatus::Failed,
                design: None,
                core_state: Some(current),
            };
        }
        let Some(candidates) = request.context.current_proposals.clone() else {
            return error_response("ExecutionError", "No active proposal", id);
        };
        CoreResponse {
            events: vec![
                CoreEvent::Proposal { candidates },
                CoreEvent::Pipeline {
                    state: PipelineState::Proposed.label().to_string(),
                },
                CoreEvent::Next {
                    actions: vec![
                        "select <n>".to_string(),
                        "compare".to_string(),
                        "cancel".to_string(),
                    ],
                },
            ],
            status: ExecutionStatus::Proposed,
            design: None,
            core_state: None,
        }
    }

    fn compare_proposals(&self, request: &InternalRequest) -> CoreResponse {
        let id = request.id;
        if observability_enabled() {
            println!("[CODING][id={}] stage=plan", id);
        }
        let Some(candidates) = request.context.current_proposals.as_ref() else {
            return error_response("ExecutionError", "No active proposal", id);
        };
        if candidates.is_empty() {
            return error_response("ExecutionError", "No active proposal", id);
        }
        let lines = candidates
            .iter()
            .map(|candidate| {
                format!(
                    "{}. score={:.2} confidence={:.2} steps={} risks={}",
                    candidate.id,
                    candidate.score,
                    candidate.confidence,
                    candidate.steps.len(),
                    candidate.risks.len()
                )
            })
            .collect::<Vec<_>>();

        CoreResponse {
            events: vec![
                CoreEvent::Debug {
                    message: lines.join("\n"),
                },
                CoreEvent::Next {
                    actions: vec!["select <n>".to_string(), "reselect".to_string()],
                },
            ],
            status: ExecutionStatus::Proposed,
            design: None,
            core_state: None,
        }
    }

    /// Navigate history one step back.  Phase 4.5.
    ///
    /// For Applied/Staged states, delegates to `rollback` (file restoration).
    /// For all other states, moves the History cursor back and returns the
    /// restored `CoreState` directly — no push to history.
    fn undo(&self, request: &InternalRequest) -> CoreResponse {
        let id = request.id;
        if observability_enabled() {
            println!("[CODING][id={}] stage=rollback", id);
        }
        if matches!(
            request.context.pipeline_state,
            PipelineState::Applied | PipelineState::Staged
        ) {
            return self.rollback(request);
        }

        let mut hist = self.history.lock().expect("history lock");
        let state_before_undo = hist.current().clone();
        if let Some(restored) = hist.undo() {
            let (mut restored, check_reason) =
                normalize_restored_state_after_undo(restored, &state_before_undo);
            let derived = derive_pipeline_state_from_context(
                &restored.session_context,
                restored.preview_snapshot.as_ref(),
            );
            restored.status = derived.clone();
            restored.previous_analysis_context =
                restored.session_context.previous_analysis_context.clone();
            hist.replace_current(restored.clone());
            let pipeline_label = derived.label().to_string();
            let version = restored.version;
            let design = restored.design.clone();
            let next_actions = next_actions_for_pipeline_state(&derived);
            drop(hist);
            CoreResponse {
                events: vec![
                    CoreEvent::Result {
                        message: format!("Undo to v{version}"),
                    },
                    CoreEvent::Debug {
                        message: format!(
                            "[IR-TRACE][ROLLBACK_STATE_CHECK] restored_state={} valid={} reason={}",
                            check_reason.restored_state.label(),
                            check_reason.valid,
                            check_reason.reason
                        ),
                    },
                    CoreEvent::Debug {
                        message: format!(
                            "[IR-TRACE][PIPELINE_DERIVE] state={} reason={}",
                            derived.label(),
                            pipeline_derive_reason(
                                &restored.session_context,
                                restored.preview_snapshot.as_ref()
                            )
                        ),
                    },
                    CoreEvent::Pipeline {
                        state: pipeline_label,
                    },
                    CoreEvent::Next {
                        actions: next_actions,
                    },
                ],
                status: ExecutionStatus::Idle,
                design,
                core_state: Some(restored),
            }
        } else {
            let current = hist.current().clone();
            drop(hist);
            CoreResponse {
                events: vec![
                    CoreEvent::Error {
                        message: "ExecutionError: nothing to undo".to_string(),
                    },
                    CoreEvent::Next {
                        actions: vec!["retry".to_string()],
                    },
                ],
                status: ExecutionStatus::Failed,
                design: None,
                core_state: Some(current),
            }
        }
    }

    /// Reset to a past version and start a new branch.  Phase 4.5.
    ///
    /// Parses an optional numeric version from `step`; omitting it re-anchors
    /// to the current cursor position.  Truncates the forward chain.
    fn replay(&self, request: &InternalRequest, step: Option<&str>) -> CoreResponse {
        let id = request.id;
        if observability_enabled() {
            println!("[CODING][id={}] stage=plan", id);
        }
        let target_version = match step.filter(|s| !s.is_empty()) {
            Some(step) => match step.parse::<u64>() {
                Ok(version) => version,
                Err(_) => {
                    let current = self.history.lock().expect("history lock").current().clone();
                    return replay_validation_failure_response(
                        current,
                        ReplayValidationFailure::ParseFailure,
                        None,
                        None,
                        None,
                    );
                }
            },
            None => {
                let current = self.history.lock().expect("history lock").current().clone();
                return replay_validation_failure_response(
                    current,
                    ReplayValidationFailure::ParseFailure,
                    None,
                    None,
                    None,
                );
            }
        };

        let mut hist = self.history.lock().expect("history lock");
        if !hist.contains_replay_target(target_version) {
            let current = hist.current().clone();
            drop(hist);
            return replay_validation_failure_response(
                current,
                ReplayValidationFailure::InvalidTarget,
                Some(target_version),
                None,
                None,
            );
        }

        let replay_distance = hist
            .replay_distance_to(target_version)
            .expect("validated replay target must have a distance");
        if replay_distance > self.limits.max_replay_steps {
            let current = hist.current().clone();
            drop(hist);
            return replay_validation_failure_response(
                current,
                ReplayValidationFailure::ReplayLimitExceeded,
                Some(target_version),
                Some(replay_distance),
                Some(self.limits.max_replay_steps),
            );
        }

        if let Some(restored) = hist.replay_from(target_version) {
            let pipeline_label = restored.status.label().to_string();
            let v = restored.version;
            let design = restored.design.clone();
            drop(hist);
            CoreResponse {
                events: vec![
                    CoreEvent::Debug {
                        message: format!(
                            "[IR-TRACE][REPLAY_TARGET_VALIDATE] target_version={target_version} exists=true"
                        ),
                    },
                    CoreEvent::Debug {
                        message: format!(
                            "[IR-TRACE][REPLAY_DISTANCE_VALIDATE] target_version={target_version} distance={replay_distance} limit={}",
                            self.limits.max_replay_steps
                        ),
                    },
                    CoreEvent::Result {
                        message: format!("Replay from v{v} (new branch)"),
                    },
                    CoreEvent::Pipeline {
                        state: pipeline_label,
                    },
                    CoreEvent::Next {
                        actions: vec!["select <n>".to_string(), "y".to_string(), "n".to_string()],
                    },
                ],
                status: ExecutionStatus::Planned,
                design,
                core_state: Some(restored),
            }
        } else {
            let current = hist.current().clone();
            drop(hist);
            CoreResponse {
                events: vec![CoreEvent::Error {
                    message: format!(
                        "ExecutionError: version {target_version} not found for replay"
                    ),
                }],
                status: ExecutionStatus::Failed,
                design: None,
                core_state: Some(current),
            }
        }
    }

    /// Move the history cursor to a specific version.  Phase 4.5.
    fn jump(&self, request: &InternalRequest, input: &str) -> CoreResponse {
        let id = request.id;
        if observability_enabled() {
            println!("[CODING][id={}] stage=plan", id);
        }
        let version_str = input
            .strip_prefix("jump ")
            .map(str::trim)
            .filter(|v| !v.is_empty());
        let Some(version_str) = version_str else {
            return error_response("ValidationError", "jump requires a version", id);
        };
        let version: u64 = match version_str.parse() {
            Ok(v) => v,
            Err(_) => {
                return error_response("ValidationError", "jump requires a numeric version", id);
            }
        };

        let mut hist = self.history.lock().expect("history lock");
        if let Some(jumped) = hist.jump(version) {
            let pipeline_label = jumped.status.label().to_string();
            let design = jumped.design.clone();
            drop(hist);
            CoreResponse {
                events: vec![
                    CoreEvent::Result {
                        message: format!("Jumped to v{version}"),
                    },
                    CoreEvent::Pipeline {
                        state: pipeline_label,
                    },
                    CoreEvent::Next {
                        actions: vec!["replay".to_string(), "undo".to_string()],
                    },
                ],
                status: ExecutionStatus::Idle,
                design,
                core_state: Some(jumped),
            }
        } else {
            let current = hist.current().clone();
            drop(hist);
            CoreResponse {
                events: vec![CoreEvent::Error {
                    message: format!("ExecutionError: version {version} not found"),
                }],
                status: ExecutionStatus::Failed,
                design: None,
                core_state: Some(current),
            }
        }
    }

    fn filter(&self, input: &str, id: u64) -> CoreResponse {
        if observability_enabled() {
            println!("[CODING][id={}] stage=plan", id);
        }
        let filter = input
            .strip_prefix("/filter ")
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let Some(filter) = filter else {
            return error_response("ValidationError", "/filter requires a target", id);
        };
        CoreResponse {
            events: vec![CoreEvent::Debug {
                message: format!("filter set: {filter}"),
            }],
            status: ExecutionStatus::Idle,
            design: None,
            core_state: None,
        }
    }

    fn preview(&self, request: &InternalRequest) -> CoreResponse {
        let id = request.id;
        if observability_enabled() {
            println!("[CODING][id={}] stage=preview", id);
        }
        if request.context.pipeline_state != PipelineState::Planned {
            trace_unsupported_operation("preview", "Preview", None, "requires Planned state");
            return error_response("ExecutionError", "preview requires Planned state", id);
        }
        let pending = self.pending_files.lock().expect("pending lock").clone();
        if pending.is_empty() {
            return error_response(
                "ValidationError",
                "no pending generated files to preview",
                id,
            );
        }

        let mut events = vec![CoreEvent::Preview {
            diff: preview_lines(&pending),
        }];
        events.extend(diff_events_from_pending(&pending));
        events.extend([
            CoreEvent::Pipeline {
                state: PipelineState::Previewed.label().to_string(),
            },
            CoreEvent::Next {
                actions: vec!["y".to_string(), "n".to_string()],
            },
        ]);

        CoreResponse {
            events,
            status: ExecutionStatus::Planned,
            design: None,
            core_state: None,
        }
    }

    fn apply(&self, request: &InternalRequest) -> CoreResponse {
        let id = request.id;
        if observability_enabled() {
            println!("[CODING][id={}] stage=apply", id);
        }
        if request.context.pipeline_state != PipelineState::Previewed {
            trace_unsupported_operation("apply", "Apply", None, "requires Previewed state");
            return error_response("ExecutionError", "apply requires Previewed state", id);
        }
        let pending = self.pending_files.lock().expect("pending lock").clone();
        if pending.is_empty() {
            return error_response("ValidationError", "no pending generated files to apply", id);
        }

        let intent = Intent::new(request.input.clone());
        let explicit_target = apply_intent_target(&intent)
            .or_else(|| (pending.len() == 1).then(|| PathBuf::from(pending[0].path.clone())));
        if let Some(target) = explicit_target.as_ref()
            && pending.len() != 1
        {
            return error_response(
                "TargetViolation",
                &format!(
                    "expected={} actual_files={:?}",
                    target.display(),
                    pending
                        .iter()
                        .map(|file| file.path.clone())
                        .collect::<Vec<_>>()
                ),
                id,
            );
        }

        let planned = match planned_applied_files(&request.context.working_dir, &pending) {
            Ok(planned) => planned,
            Err(err) => return error_response("SafetyViolation", &err, id),
        };
        let change_set = match pending_change_set(&request.context.working_dir, &pending) {
            Ok(change_set) => change_set,
            Err(err) => return error_response("ExecutionError", &err, id),
        };
        if observability_enabled() {
            println!(
                "[CODING] diff_files={:?}",
                change_set
                    .changes
                    .iter()
                    .map(|change| change.file_path.clone())
                    .collect::<Vec<_>>()
            );
        }

        let execution = match execute_code_change_set(
            &request.context.working_dir,
            &change_set,
            &CodingOptions {
                apply: true,
                check: true,
                no_build: true,
                backup: true,
                format: false,
                safe_mode: true,
                auto_commit: false,
                confirm_commit: false,
                prompt_commit: false,
                auto_push: false,
                confirm_push: false,
                auto_pr: false,
                confirm_pr: false,
                pr_base: "main".to_string(),
                patch_scope: if explicit_target.is_some() {
                    PatchScope::ExplicitTargetOnly
                } else {
                    PatchScope::WorkspaceWide
                },
                explicit_target: explicit_target.clone(),
            },
            None,
        ) {
            Ok(execution) => execution,
            Err(err) => return error_response("ExecutionError", &err, id),
        };

        if execution.status == "failed" {
            return error_response(
                "ExecutionError",
                execution.reason.as_deref().unwrap_or("apply failed"),
                id,
            );
        }

        let applied = if execution.status == "noop" {
            Vec::new()
        } else {
            match verify_applied_files(&pending, planned) {
                Ok(applied) => applied,
                Err(err) => return error_response("ApplyMismatch", &err, id),
            }
        };

        *self.applied_files.lock().expect("applied lock") = applied.clone();
        if observability_enabled() {
            if execution.status == "noop" {
                println!("[CODING][id={}] stage=apply status=NoOp changes=0", id);
            } else {
                println!(
                    "[CODING][id={}] stage=apply status=Applied changes={} files={}",
                    id,
                    applied.len(),
                    applied.len()
                );
            }
        }
        let _snapshot = sync_pipeline_with_git(&request.context.working_dir)
            .unwrap_or_else(|_| GitSnapshot::from_applied(&applied));
        CoreResponse {
            events: vec![
                CoreEvent::Result {
                    message: "Changes applied".to_string(),
                },
                CoreEvent::Pipeline {
                    state: PipelineState::Applied.label().to_string(),
                },
                CoreEvent::Next {
                    actions: vec!["git add <file>".to_string(), "commit changes".to_string()],
                },
            ],
            status: ExecutionStatus::Executed,
            design: None,
            core_state: None,
        }
    }

    fn cancel(&self, request: &InternalRequest) -> CoreResponse {
        let id = request.id;
        if request.context.pipeline_state != PipelineState::Proposed
            && request.context.pipeline_state != PipelineState::Previewed
            && request.context.pipeline_state != PipelineState::Planned
        {
            trace_unsupported_operation("cancel", "Cancel", None, "requires active proposal/plan");
            return error_response(
                "ExecutionError",
                "cancel requires active proposal or preview",
                id,
            );
        }
        self.pending_files.lock().expect("pending lock").clear();
        CoreResponse {
            events: vec![
                CoreEvent::Result {
                    message: "Cancelled".to_string(),
                },
                CoreEvent::Pipeline {
                    state: PipelineState::Idle.label().to_string(),
                },
                CoreEvent::Next { actions: vec![] },
            ],
            status: ExecutionStatus::Idle,
            design: None,
            core_state: None,
        }
    }

    fn git_add(&self, request: &InternalRequest, input: &str) -> CoreResponse {
        let id = request.id;
        if observability_enabled() {
            println!("[CODING][id={}] stage=apply", id);
        }
        if request.context.pipeline_state != PipelineState::Applied {
            trace_unsupported_operation(input, "GitAdd", None, "requires Applied state");
            return error_response("ExecutionError", "git add requires Applied state", id);
        }
        let Some(path) = input.strip_prefix("git add ").map(str::trim) else {
            return error_response("ValidationError", "git add requires one explicit file", id);
        };
        if let Err(err) = validate_git_add_path(path) {
            trace_unsupported_operation(input, "GitAdd", Some(path), &err);
            return error_response("SafetyViolation", &err, id);
        }
        let mut transaction =
            ExecutionTransaction::new(format!("tx-{id}"), timestamp_millis() as u64);
        transaction.mark_applied();
        if let Err(err) = reject_dirty_worktree_except(
            &request.context.working_dir,
            &DirtyTreePolicy::default(),
            Some(Path::new(path)),
        ) {
            transaction.mark_git_failed(GitPhase::Add, err.clone());
            return CoreResponse {
                events: vec![
                    CoreEvent::Debug {
                        message: transaction_record_json(&transaction),
                    },
                    CoreEvent::Error {
                        message: format!("ExecutionError: {err}"),
                    },
                ],
                status: ExecutionStatus::Failed,
                design: None,
                core_state: None,
            };
        }
        match git_add_file(&request.context.working_dir, Path::new(path)) {
            Ok(_) => {}
            Err(err) => {
                transaction.mark_git_failed(GitPhase::Add, err.clone());
                return CoreResponse {
                    events: vec![
                        CoreEvent::Debug {
                            message: transaction_record_json(&transaction),
                        },
                        CoreEvent::Error {
                            message: format!("ExecutionError: {err}"),
                        },
                    ],
                    status: ExecutionStatus::Failed,
                    design: None,
                    core_state: None,
                };
            }
        }
        let snapshot = match sync_pipeline_with_git(&request.context.working_dir) {
            Ok(snapshot) => snapshot,
            Err(err) => return error_response("ExecutionError", &err, id),
        };
        if snapshot.staged.is_empty() {
            return error_response("ExecutionError", "git add produced no staged changes", id);
        }
        transaction.mark_staged(snapshot.staged.iter().map(PathBuf::from).collect());
        CoreResponse {
            events: vec![
                CoreEvent::Debug {
                    message: transaction_record_json(&transaction),
                },
                CoreEvent::Pipeline {
                    state: PipelineState::Staged.label().to_string(),
                },
                CoreEvent::Result {
                    message: format!("Staged: {path}"),
                },
                CoreEvent::Next {
                    actions: vec!["commit changes".to_string()],
                },
            ],
            status: ExecutionStatus::Executed,
            design: None,
            core_state: None,
        }
    }

    fn git_commit(&self, request: &InternalRequest) -> CoreResponse {
        let id = request.id;
        if observability_enabled() {
            println!("[CODING][id={}] stage=apply", id);
        }
        if request.context.pipeline_state != PipelineState::Staged {
            trace_unsupported_operation("git commit", "GitCommit", None, "requires Staged state");
            return error_response("ExecutionError", "commit requires Staged state", id);
        }
        let mut transaction =
            ExecutionTransaction::new(format!("tx-{id}"), timestamp_millis() as u64);
        transaction.mark_applied();
        let staged = match git_lines(
            &request.context.working_dir,
            &["diff", "--cached", "--name-only"],
        ) {
            Ok(files) if !files.is_empty() => files,
            Ok(_) => {
                return error_response("ExecutionError", "git commit requires staged changes", id);
            }
            Err(err) => return error_response("ExecutionError", &err, id),
        };
        transaction.mark_staged(staged.iter().map(PathBuf::from).collect());
        if let Err(err) =
            reject_unstaged_worktree(&request.context.working_dir, &DirtyTreePolicy::default())
        {
            transaction.mark_git_failed(GitPhase::Commit, err.clone());
            return CoreResponse {
                events: vec![
                    CoreEvent::Debug {
                        message: transaction_record_json(&transaction),
                    },
                    CoreEvent::Error {
                        message: format!("ExecutionError: {err}"),
                    },
                ],
                status: ExecutionStatus::Failed,
                design: None,
                core_state: None,
            };
        }
        let hash = match git_commit_fixed(&request.context.working_dir) {
            Ok(hash) => hash,
            Err(err) => {
                transaction.mark_git_failed(GitPhase::Commit, err.clone());
                return CoreResponse {
                    events: vec![
                        CoreEvent::Debug {
                            message: transaction_record_json(&transaction),
                        },
                        CoreEvent::Error {
                            message: format!("ExecutionError: {err}"),
                        },
                    ],
                    status: ExecutionStatus::Failed,
                    design: None,
                    core_state: None,
                };
            }
        };
        transaction.mark_committed(hash.clone());
        transaction.finalize(timestamp_millis() as u64);
        let _snapshot = sync_pipeline_with_git(&request.context.working_dir).unwrap_or_default();
        CoreResponse {
            events: vec![
                CoreEvent::Debug {
                    message: transaction_record_json(&transaction),
                },
                CoreEvent::Result {
                    message: format!("Committed: {hash}"),
                },
                CoreEvent::Pipeline {
                    state: PipelineState::Committed.label().to_string(),
                },
                CoreEvent::Next {
                    actions: vec!["continue development".to_string()],
                },
            ],
            status: ExecutionStatus::Executed,
            design: None,
            core_state: None,
        }
    }

    fn rollback(&self, request: &InternalRequest) -> CoreResponse {
        let id = request.id;
        if observability_enabled() {
            println!("[CODING][id={}] stage=rollback", id);
        }
        if request.context.pipeline_state == PipelineState::Committed {
            trace_unsupported_operation("rollback", "Rollback", None, "committed state");
            return error_response("ExecutionError", "RollbackForbidden", id);
        }
        let applied = self.applied_files.lock().expect("applied lock").clone();
        if applied.is_empty() {
            return error_response("ExecutionError", "no applied changes to rollback", id);
        }
        restore_applied(&applied);
        self.applied_files.lock().expect("applied lock").clear();
        CoreResponse {
            events: vec![
                CoreEvent::Result {
                    message: "Rollback completed".to_string(),
                },
                CoreEvent::Pipeline {
                    state: PipelineState::Previewed.label().to_string(),
                },
                CoreEvent::Next {
                    actions: vec!["y".to_string(), "n".to_string()],
                },
            ],
            status: ExecutionStatus::Planned,
            design: None,
            core_state: None,
        }
    }

    fn store_pending_files(&self, result: &runtime_core::stable_v03::RuntimeResult) {
        let files = result
            .project_layout
            .files
            .iter()
            .map(|file| PendingFile {
                path: file.path.clone(),
                content: file.content.clone(),
                content_checksum: checksum_bytes(file.content.as_bytes()),
            })
            .collect::<Vec<_>>();
        *self.pending_files.lock().expect("pending lock") = files;
        self.applied_files.lock().expect("applied lock").clear();
    }

    /// Handle `select <n>` — pick a proposal candidate and transition the
    /// pipeline through Planned → Previewed.  Phase 1C.5 §7.1.
    fn select_candidate(&self, request: &InternalRequest, input: &str) -> CoreResponse {
        let id = request.id;
        if observability_enabled() {
            println!("[CODING][id={}] stage=plan", id);
        }
        if self.current_depth() >= self.limits.max_depth {
            return self.max_depth_response();
        }

        // §5.2 制約: select requires Proposed state
        if request.context.pipeline_state != PipelineState::Proposed {
            trace_unsupported_operation(input, "Select", None, "requires Proposed state");
            return error_response("ExecutionError", "Cannot select in current state", id);
        }

        // §9.2 Proposal未存在
        let Some(proposals) = request.context.current_proposals.as_ref() else {
            return error_response("ExecutionError", "No active proposal", id);
        };
        if proposals.is_empty() {
            return error_response("ExecutionError", "No active proposal", id);
        }

        // §3.1 parse 1-based index
        let index_str = input.strip_prefix("select ").map(str::trim).unwrap_or("");
        let index: usize = match index_str.parse::<usize>() {
            Ok(n) if n >= 1 => n,
            _ => return error_response("ExecutionError", "Invalid selection index", id),
        };

        // §9.1 bound check
        let Some(candidate) = proposals.get(index - 1) else {
            return error_response("ExecutionError", "Invalid selection index", id);
        };

        // §11 IR-TRACE
        trace_ir!(
            TraceLevel::Basic,
            "SELECT",
            format!("candidate_id={}", candidate.id)
        );

        // §6 candidate → execution plan
        let plan = match candidate_to_execution_plan(candidate) {
            Ok(plan) => plan,
            Err(err) => return error_response("ValidationError", &err, id),
        };

        trace_ir!(
            TraceLevel::Basic,
            "PLAN",
            format!("steps={}", plan.steps.join(", "))
        );

        let selected_target = request.context.current_proposals.as_ref().and_then(|_| {
            self.history
                .lock()
                .expect("history lock")
                .current()
                .session_context
                .previous_plan_context
                .as_ref()
                .and_then(|ctx| {
                    ctx.candidates
                        .iter()
                        .find(|plan_candidate| plan_candidate.candidate_id == index)
                        .map(|plan_candidate| plan_candidate.target.clone())
                })
        });

        // Build preview diff from the selected candidate and keep that exact
        // pending content as the only source Safe Apply may write.
        let pending = {
            let mut pending = self.pending_files.lock().expect("pending lock");
            if pending.is_empty()
                && let Some(NarrowTarget::File(path)) = selected_target.as_ref()
            {
                match safe_apply_preview_file(&request.context.working_dir, path, index) {
                    Ok(file) => *pending = vec![file],
                    Err(err) => return error_response("ValidationError", &err, id),
                }
            }
            pending.clone()
        };
        let preview = if pending.is_empty() {
            vec!["(no pending files)".to_string()]
        } else {
            preview_lines(&pending)
        };
        let preview_hash = checksum_u64(preview.join("\n").as_bytes());
        let mut events = vec![
            CoreEvent::Plan {
                steps: std::iter::once(format!("Selected: {}", candidate.summary))
                    .chain(plan.steps.iter().cloned())
                    .collect(),
            },
            CoreEvent::Pipeline {
                state: PipelineState::Planned.label().to_string(),
            },
            CoreEvent::Preview { diff: preview },
        ];
        events.extend(diff_events_from_pending(&pending));
        events.extend([
            CoreEvent::Result {
                message: format!("Selected: {}", candidate.summary),
            },
            CoreEvent::Debug {
                message: format!(
                    "[IR-TRACE][CONTEXT_STORE] kind=selection candidate_id={} target={}",
                    index,
                    candidate
                        .target
                        .as_ref()
                        .map(|target| format!("File({})", target.file))
                        .unwrap_or_else(|| "None".to_string())
                ),
            },
            CoreEvent::Pipeline {
                state: PipelineState::Previewed.label().to_string(),
            },
            CoreEvent::Next {
                actions: vec!["y".to_string(), "n".to_string()],
            },
        ]);

        {
            let mut history = self.history.lock().expect("history lock");
            let mut next_state = history.current().clone();
            if let Some(selected_target) = selected_target {
                next_state
                    .session_context
                    .store_selection(index, selected_target);
                if let Some(selected) = next_state.session_context.selected_candidate.as_ref() {
                    next_state.preview_snapshot = Some(PreviewSnapshot {
                        candidate_id: selected.candidate_id,
                        target: selected.target.clone(),
                        plan_hash: selected.plan_hash,
                        preview_hash,
                        created_at: current_timestamp_secs_core(),
                    });
                }
            }
            next_state.previous_analysis_context =
                next_state.session_context.previous_analysis_context.clone();
            history.push(next_state);
        }

        // Emit: Plan → Pipeline::Planned → Preview → Result → Pipeline::Previewed → Next
        // The double Pipeline emission walks through each required state step.  §5.2
        CoreResponse {
            events,
            status: ExecutionStatus::Planned,
            design: None,
            core_state: None,
        }
    }
}

fn core_events_from_strategy(output: &StrategyOutput) -> Vec<CoreEvent> {
    let mut events = Vec::new();

    if !output.selected_plan.build_plan.build_commands.is_empty()
        || !output.selected_plan.test_plan.test_commands.is_empty()
    {
        let mut steps = output
            .selected_plan
            .build_plan
            .build_commands
            .iter()
            .map(|cmd| format!("build: {cmd}"))
            .collect::<Vec<_>>();
        steps.extend(
            output
                .selected_plan
                .test_plan
                .test_commands
                .iter()
                .map(|cmd| format!("test: {cmd}")),
        );
        events.push(CoreEvent::Plan { steps });
    }

    for attempt in &output.strategy_trace.attempts {
        events.push(CoreEvent::Editing {
            target: format!("{:?}", attempt.strategy_kind),
            action: if attempt.success {
                "accepted execution plan".to_string()
            } else {
                "retry required".to_string()
            },
            reason: attempt
                .failure_context
                .as_ref()
                .map(|failure| format!("{:?}", failure.error)),
        });
    }
    events
}

fn log_clear_intent_runtime_clarification_bypassed(
    events: &mut Vec<CoreEvent>,
    intent: &Intent,
    input: &str,
    clarification: &runtime_core::Clarification,
) -> Option<String> {
    if requires_clarification(intent) {
        return None;
    }
    if !matches!(
        intent.action,
        Action::Fix | Action::Improve | Action::Optimize | Action::RefactorGeneric
    ) {
        return None;
    }

    let target = clarification_target_from_intent_or_input(intent, input);
    let target = target?;
    let action = format!("{:?}", intent.action);

    trace_ir!(
        TraceLevel::Basic,
        "CLARIFICATION_BYPASSED",
        format!(
            "target={target}, action={action}, runtime_message={}",
            clarification.message
        )
    );

    events.push(trace_event(
        "CLARIFICATION_BYPASSED",
        json!({
            "target": target,
            "action": action,
            "runtime_message": clarification.message,
            "timestamp": timestamp_millis(),
        }),
    ));

    Some(target)
}

fn clarification_target_from_intent_or_input(intent: &Intent, input: &str) -> Option<String> {
    let target = intent
        .file
        .clone()
        .or_else(|| intent.symbol.clone())
        .or_else(|| intent.target.clone());
    match target.as_deref() {
        Some(".") | None => input
            .split_whitespace()
            .find(|token| {
                Path::new(token)
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .is_some()
            })
            .map(|token| {
                token
                    .trim_matches(|c: char| c == ',' || c == '.')
                    .to_string()
            })
            .or(target),
        _ => target,
    }
}

fn append_comment_line(content: &str, target: &str) -> String {
    let marker = match Path::new(target).extension().and_then(|ext| ext.to_str()) {
        Some("py") => "# DBM clarification execution guarantee",
        Some("toml") | Some("yaml") | Some("yml") => "# DBM clarification execution guarantee",
        _ => "// DBM clarification execution guarantee",
    };
    if content.lines().any(|line| line.trim() == marker) {
        return content.to_string();
    }
    let mut updated = content.to_string();
    if !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(marker);
    updated.push('\n');
    updated
}

fn resolve_clarification_target(root: &Path, target: &str) -> Result<(String, PathBuf), String> {
    let direct = resolve_repo_file(root, target)?;
    if direct.is_file() {
        return Ok((target.to_string(), direct));
    }

    let target_path = Path::new(target);
    if target_path.is_relative() {
        let cli_relative = Path::new("apps").join("cli").join(target_path);
        let cli_relative_str = cli_relative.to_string_lossy().to_string();
        let cli_target = resolve_repo_file(root, &cli_relative_str)?;
        if cli_target.is_file() {
            return Ok((cli_relative_str, cli_target));
        }
    }

    Ok((target.to_string(), direct))
}

fn design_document_from_core_result(
    result: &runtime_core::stable_v03::RuntimeResult,
    strategy_output: &StrategyOutput,
    previous: Option<&DesignDocument>,
) -> DesignDocument {
    let version = previous
        .map(|doc| doc.version.saturating_add(1))
        .unwrap_or(1);
    let mut reason_units = Vec::new();
    if let Some(trace) = &result.reasoning_trace {
        for step in &trace.steps {
            reason_units.push(ReasonUnit {
                id: format!("reason-depth-{}", step.depth),
                title: format!("depth {}", step.depth),
                summary: format!(
                    "beam={} candidates={} pruned={} recall_hits={}",
                    step.beam_width, step.candidates, step.pruned, step.recall_hits
                ),
            });
        }
    }
    if reason_units.is_empty() {
        reason_units.push(ReasonUnit {
            id: "strategy".to_string(),
            title: "strategy".to_string(),
            summary: strategy_output.strategy_trace.final_outcome.to_string(),
        });
    }

    let functions = result
        .project_layout
        .files
        .iter()
        .take(12)
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();

    DesignDocument::new(
        version,
        reason_units,
        StructureTree {
            module: result.project_layout.root_dir.clone(),
            functions,
        },
        vec![
            Constraint {
                text: "Validation passed before design reflection".to_string(),
            },
            Constraint {
                text: format!(
                    "strategy outcome: {}",
                    strategy_output.strategy_trace.final_outcome
                ),
            },
        ],
    )
}

fn trace_enabled(level: TraceLevel) -> bool {
    TRACE_LEVEL >= level && TRACE_LEVEL != TraceLevel::Off
}

fn trace_event(stage: &str, data: serde_json::Value) -> CoreEvent {
    let rendered = ir_trace_json(stage, data);
    trace_ir!(TraceLevel::Basic, stage, rendered);
    CoreEvent::Debug {
        message: format!("[DETAIL]\n[{stage}] {rendered}"),
    }
}

fn ir_trace_json(stage: &str, data: serde_json::Value) -> String {
    json!({
        "stage": stage,
        "data": data,
        "timestamp": timestamp_millis(),
    })
    .to_string()
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn format_capability_result(
    result: &dyn std::any::Any,
    output_type: OutputTypeId,
    path: &str,
) -> String {
    match output_type {
        OutputTypeId::ProjectStructureAnalysisResult => {
            let res = result
                .downcast_ref::<ProjectStructureAnalysisResult>()
                .expect("result must be ProjectStructureAnalysisResult");
            format_project_structure_analysis_text(path, res)
        }
        OutputTypeId::TestInventoryResult => {
            let res = result
                .downcast_ref::<TestInventoryResult>()
                .expect("result must be TestInventoryResult");
            format_test_inventory_text(path, res)
        }
        OutputTypeId::CodeAnalysisResult => {
            let res = result
                .downcast_ref::<CodeAnalysisResult>()
                .expect("result must be CodeAnalysisResult");
            format!("# Code Analysis\n\n- Path: `{path}`\n\n{}", res.summary)
        }
        OutputTypeId::MemoryAnalysisResult => {
            let res = result
                .downcast_ref::<MemoryAnalysisResult>()
                .expect("result must be MemoryAnalysisResult");
            format!("# Memory Analysis\n\n- Path: `{path}`\n\n{}", res.summary)
        }
    }
}

fn format_project_structure_analysis_text(path: &str, result: &ProjectStructureAnalysisResult) -> String {
    let mut out = String::new();
    out.push_str("# Project Structure Analysis\n\n");
    out.push_str(&format!("- Path: `{path}`\n"));
    out.push_str(&format!("- Summary: {}\n", result.summary));
    if !result.modules.is_empty() {
        out.push_str("\n## Modules\n\n");
        for module in result.modules.iter().take(20) {
            out.push_str(&format!("- `{module}`\n"));
        }
        if result.modules.len() > 20 {
            out.push_str(&format!("- ... and {} more\n", result.modules.len() - 20));
        }
    }
    out
}

fn format_test_inventory_text(path: &str, result: &TestInventoryResult) -> String {
    let mut out = String::new();
    out.push_str("# Test Inventory Result\n\n");
    out.push_str(&format!("- Path: `{path}`\n"));
    out.push_str(&format!("- Total Tests Found: {}\n", result.test_count));
    out.push_str(&format!("- Summary: {}\n", result.summary));
    if !result.test_files.is_empty() {
        out.push_str("\n## Test Files\n\n");
        for file in result.test_files.iter().take(20) {
            out.push_str(&format!("- `{file}`\n"));
        }
        if result.test_files.len() > 20 {
            out.push_str(&format!("- ... and {} more\n", result.test_files.len() - 20));
        }
    }
    out
}

/// 修正プランの候補を Markdown テキストに整形する。
/// Spec DBM-CONTEXT-AWARE-PLAN-TARGET-RESOLUTION-SPEC v1.0 §11
fn generate_narrow_change_candidates(root: &Path) -> Vec<ChangePlanCandidate> {
    let preferred = [
        "apps/cli/src/core.rs",
        "apps/cli/src/repl.rs",
        "apps/cli/src/nl/language_core_ir_adapter.rs",
        "src/core.rs",
        "src/repl.rs",
        "src/nl/language_core_ir_adapter.rs",
        "README.md",
        "Cargo.toml",
    ];
    let mut files = preferred
        .iter()
        .filter(|path| root.join(path).is_file())
        .map(|path| (*path).to_string())
        .collect::<Vec<_>>();
    if files.len() < 3 {
        files.extend(
            ["Cargo.toml", "README.md", "src/lib.rs"]
                .iter()
                .filter(|path| root.join(path).is_file())
                .map(|path| (*path).to_string()),
        );
    }
    files.sort();
    files.dedup();
    files
        .into_iter()
        .take(5)
        .enumerate()
        .map(|(index, file)| ChangePlanCandidate {
            candidate_id: index + 1,
            title: format!("Small safe cleanup in {file}"),
            target: NarrowTarget::File(file.clone()),
            proposed_change: "Add a focused, non-destructive maintainability improvement after review.".to_string(),
            rationale: "The target is a concrete file inside the workspace, avoiding WorkspaceRoot mutation.".to_string(),
            risk_level: if index == 0 {
                crate::runtime::autonomous_control::RiskLevel::Low
            } else {
                crate::runtime::autonomous_control::RiskLevel::Medium
            },
            requires_validation: true,
        })
        .collect()
}

fn execution_candidates_from_change_candidates(
    candidates: &[ChangePlanCandidate],
) -> Vec<ExecutionPlanCandidate> {
    candidates
        .iter()
        .map(|candidate| {
            let file = match &candidate.target {
                NarrowTarget::File(path) => path.clone(),
                NarrowTarget::Module(module) => module.clone(),
                NarrowTarget::Symbol(symbol) => symbol.clone(),
            };
            ExecutionPlanCandidate::from_ops(
                candidate.candidate_id,
                candidate.title.clone(),
                vec![strategy_engine::ExecutionOp::RuntimePhase(
                    candidate.proposed_change.clone(),
                )],
                Some(ResolvedTarget { file, symbol: None }),
            )
        })
        .collect()
}

fn format_change_plan(
    ir_request: &crate::nl::language_core_ir_adapter::IrIntentRequest,
    candidates: &[ChangePlanCandidate],
) -> String {
    let mut out = String::new();
    out.push_str("# Change Plan\n\n");
    out.push_str(&format!("Mode: {}\n", ir_request.mode));
    out.push_str(&format!("Scope: {}\n", ir_request.target));
    out.push_str("Apply status: Not applied\n\n");

    out.push_str("## Candidates\n\n");
    if candidates.is_empty() {
        out.push_str("No suitable change candidates found for the current target.\n");
    } else {
        for candidate in candidates {
            out.push_str(&format!(
                "{}. {}\n   Target: {}\n   Proposed change: {}\n   Rationale: {}\n   Risk: {:?}\n   Validation required: {}\n\n",
                candidate.candidate_id,
                candidate.title,
                candidate.target,
                candidate.proposed_change,
                candidate.rationale,
                candidate.risk_level,
                if candidate.requires_validation { "yes" } else { "no" }
            ));
        }
    }

    out.push_str("\n## Safety\n");
    out.push_str("- No files modified\n");
    out.push_str("- No apply executed\n");
    if ir_request.target == crate::nl::language_core_ir_adapter::IrTarget::WorkspaceRoot {
        out.push_str("- WorkspaceRoot apply prohibited\n");
    }

    out.push_str("\n## Next\n");
    out.push_str("- Select one candidate target before apply (`select <n>`)\n");
    out
}

fn is_plan_validation_intent(input: &str) -> bool {
    let lower = input.to_lowercase();
    lower.contains("検証して")
        || lower.contains("候補を検証")
        || lower.contains("修正プランを検証")
        || lower.contains("validate selected plan")
        || lower.contains("validate this candidate")
}

fn validate_change_plan_candidate(
    root: &Path,
    plan_hash: u64,
    candidate: &ChangePlanCandidate,
    selected: &crate::nl::context_aware_plan_target_resolver::SelectedCandidateContext,
) -> PlanValidationResult {
    if selected.plan_hash != plan_hash {
        return PlanValidationResult {
            plan_hash,
            candidate_id: candidate.candidate_id,
            target: candidate.target.clone(),
            status: ValidationStatus::Rejected,
            risk_level: candidate.risk_level,
            apply_allowed: false,
            reason: "Plan hash mismatch.".to_string(),
        };
    }
    let target_exists = match &candidate.target {
        NarrowTarget::File(path) => {
            let joined = root.join(path);
            joined.is_file() && joined.starts_with(root)
        }
        NarrowTarget::Module(_) | NarrowTarget::Symbol(_) => true,
    };
    if !target_exists {
        return PlanValidationResult {
            plan_hash,
            candidate_id: candidate.candidate_id,
            target: candidate.target.clone(),
            status: ValidationStatus::Rejected,
            risk_level: candidate.risk_level,
            apply_allowed: false,
            reason: "Target is missing or outside workspace.".to_string(),
        };
    }
    if candidate.risk_level > crate::runtime::autonomous_control::RiskLevel::Medium {
        return PlanValidationResult {
            plan_hash,
            candidate_id: candidate.candidate_id,
            target: candidate.target.clone(),
            status: ValidationStatus::ReviewRequired,
            risk_level: candidate.risk_level,
            apply_allowed: false,
            reason: "Risk is above Medium and requires review.".to_string(),
        };
    }
    PlanValidationResult {
        plan_hash,
        candidate_id: candidate.candidate_id,
        target: candidate.target.clone(),
        status: ValidationStatus::Passed,
        risk_level: candidate.risk_level,
        apply_allowed: true,
        reason: "Target is narrow and non-destructive.".to_string(),
    }
}

fn format_plan_validation(result: &PlanValidationResult) -> String {
    format!(
        "# Plan Validation\n\nCandidate: {}\nTarget: {}\nStatus: {:?}\nApply allowed: {}\nReason: {}",
        result.candidate_id, result.target, result.status, result.apply_allowed, result.reason
    )
}

fn format_validation_review(
    validated: &ValidatedPlanContext,
    previous_validation: Option<&PreviousValidationContext>,
    no_apply: bool,
    rejected_confirmation_like_target: bool,
) -> String {
    let status = previous_validation
        .map(|ctx| format!("{:?}", ctx.validation_status))
        .unwrap_or_else(|| "Passed".to_string());
    let reason = if rejected_confirmation_like_target {
        "confirmation-like token was rejected as target; Core fallback priority preserved review flow."
    } else if no_apply {
        "no_apply constraint is active."
    } else {
        "Apply was not explicitly confirmed."
    };
    format!(
        "# Validation Review\n\nCandidate: {}\nTarget: {}\nStatus: {}\nApply allowed: {}\nApply status: Deferred\nReason: {}\n\n## Safety\n- yes/no treated as referenced text, not confirmation\n- y/n treated as referenced text, not confirmation\n- No files modified\n- No apply executed\n- Existing validated_plan preserved\n- Awaiting explicit confirmation",
        validated.candidate_id, validated.target, status, validated.apply_allowed, reason
    )
}

fn format_safety_review(no_apply: bool, rejected_confirmation_like_target: bool) -> String {
    let target_extraction = if rejected_confirmation_like_target {
        "Rejected confirmation-like token"
    } else {
        "None"
    };
    format!(
        "# Safety Review\n\nStatus: Reviewed\nConfirmation handling: Not triggered\nTarget extraction: {target_extraction}\nApply status: Deferred\n\n## Safety\n- yes/no treated as referenced text, not confirmation\n- y/n treated as referenced text, not confirmation\n- No files modified\n- No apply executed\n- no_apply constraint: {}",
        if no_apply { "active" } else { "inactive" },
    )
}

fn fallback_for_confirmation_like_target_failure(
    state: &CoreState,
    safety: &crate::nl::language_core_ir_adapter::SafetyConstraints,
    raw_target: &str,
) -> crate::nl::language_core_ir_adapter::IrIntentRequest {
    use crate::nl::language_core_ir_adapter::IrIntentRequest;
    use crate::nl::normalization::TargetResolutionFailure;

    let session = &state.session_context;
    let (action, mode, target) = if session.validated_plan.is_some() {
        let target = session
            .selected_candidate
            .as_ref()
            .map(|selected| narrow_target_to_ir_target(&selected.target))
            .or_else(|| {
                session
                    .validated_plan
                    .as_ref()
                    .map(|validated| narrow_target_to_ir_target(&validated.target))
            })
            .unwrap_or(IrTarget::None);
        (
            IrAction::ReviewValidatedPlan,
            ExecutionMode::ValidateOnly,
            target,
        )
    } else if let Some(selected) = session.selected_candidate.as_ref() {
        (
            IrAction::ValidatePlan,
            ExecutionMode::ValidateOnly,
            narrow_target_to_ir_target(&selected.target),
        )
    } else {
        (
            IrAction::ReviewSafety,
            ExecutionMode::ReadOnly,
            IrTarget::None,
        )
    };

    IrIntentRequest {
        action,
        target,
        mode,
        raw_input: raw_target.to_string(),
        confidence: 0.85,
        safety_constraints: *safety,
        target_failure: Some(TargetResolutionFailure::ConfirmationTokenLike {
            raw: raw_target.to_string(),
        }),
    }
}

fn push_confirmation_like_fallback_trace(
    events: &mut Vec<CoreEvent>,
    session: &crate::nl::context_aware_plan_target_resolver::ReplSessionContext,
) {
    events.push(CoreEvent::Debug {
        message:
            "[IR-TRACE][FALLBACK_PRIORITY] reason=ConfirmationTokenLike before=UnresolvedTarget"
                .to_string(),
    });
    if let Some(validated) = session.validated_plan.as_ref() {
        events.push(CoreEvent::Debug {
            message:
                "[IR-TRACE][INTENT_DOWNGRADE] to=ReviewValidatedPlan reason=RejectedConfirmationLikeTargetWithValidatedPlan"
                    .to_string(),
        });
        events.push(CoreEvent::Debug {
            message: format!(
                "[IR-TRACE][VALIDATED_PLAN_PRESERVE] candidate_id={} target={}",
                validated.candidate_id, validated.target
            ),
        });
    } else if session.selected_candidate.is_some() {
        events.push(CoreEvent::Debug {
            message:
                "[IR-TRACE][INTENT_DOWNGRADE] to=ValidatePlan reason=RejectedConfirmationLikeTargetWithSelectedCandidate"
                    .to_string(),
        });
    } else {
        events.push(CoreEvent::Debug {
            message:
                "[IR-TRACE][INTENT_DOWNGRADE] to=ReviewSafety reason=RejectedConfirmationLikeTargetWithoutContext"
                    .to_string(),
        });
    }
}

fn narrow_target_to_ir_target(target: &NarrowTarget) -> IrTarget {
    match target {
        NarrowTarget::File(path) => IrTarget::File(path.clone()),
        NarrowTarget::Module(name) | NarrowTarget::Symbol(name) => IrTarget::Symbol(name.clone()),
    }
}

pub fn derive_pipeline_state_from_context(
    context: &ReplSessionContext,
    preview_snapshot: Option<&PreviewSnapshot>,
) -> PipelineState {
    match (
        context.validated_plan.as_ref(),
        context.selected_candidate.as_ref(),
        context.previous_plan_context.as_ref(),
        context.previous_analysis_context.as_ref(),
        preview_snapshot,
    ) {
        (Some(_), _, _, _, Some(_)) => PipelineState::Previewed,
        (_, Some(_), _, _, Some(_)) => PipelineState::Previewed,
        (_, None, Some(_), _, _) => PipelineState::Proposed,
        (_, None, None, Some(_), _) => PipelineState::Idle,
        _ => PipelineState::Idle,
    }
}

fn pipeline_derive_reason(
    context: &ReplSessionContext,
    preview_snapshot: Option<&PreviewSnapshot>,
) -> &'static str {
    match (
        context.validated_plan.as_ref(),
        context.selected_candidate.as_ref(),
        context.previous_plan_context.as_ref(),
        context.previous_analysis_context.as_ref(),
        preview_snapshot,
    ) {
        (Some(_), _, _, _, Some(_)) => "ValidatedPlanWithPreview",
        (_, Some(_), _, _, Some(_)) => "SelectedCandidateWithPreview",
        (_, None, Some(_), _, _) => "PreviousPlanWithoutSelection",
        (_, None, None, Some(_), _) => "PreviousAnalysisWithoutPlan",
        _ => "NoContext",
    }
}

#[derive(Debug, Clone)]
struct RollbackStateCheck {
    restored_state: PipelineState,
    valid: bool,
    reason: &'static str,
}

fn normalize_restored_state_after_undo(
    mut restored: CoreState,
    state_before_undo: &CoreState,
) -> (CoreState, RollbackStateCheck) {
    let restored_state = restored.status.clone();
    let mut valid = restored_state != PipelineState::Previewed
        || preview_state_is_valid(
            &restored.session_context,
            restored.preview_snapshot.as_ref(),
        );
    let mut reason = if valid {
        "Valid"
    } else {
        preview_invalid_reason(&restored)
    };

    let undoing_from_cleared_preview = matches!(restored_state, PipelineState::Previewed)
        && state_before_undo.preview_snapshot.is_none()
        && state_before_undo
            .session_context
            .selected_candidate
            .is_none()
        && state_before_undo.session_context.validated_plan.is_none();
    if undoing_from_cleared_preview {
        restored.preview_snapshot = None;
        restored.session_context.selected_candidate = None;
        restored.session_context.validated_plan = None;
        restored.session_context.previous_validation_context = None;
        valid = false;
        reason = "PreviewClearedBeforeUndo";
    }

    if !valid {
        restored.status = derive_pipeline_state_from_context(
            &restored.session_context,
            restored.preview_snapshot.as_ref(),
        );
    }

    (
        restored,
        RollbackStateCheck {
            restored_state,
            valid,
            reason,
        },
    )
}

fn preview_state_is_valid(
    context: &ReplSessionContext,
    preview_snapshot: Option<&PreviewSnapshot>,
) -> bool {
    let Some(snapshot) = preview_snapshot else {
        return false;
    };
    if let Some(validated) = context.validated_plan.as_ref()
        && validated.candidate_id == snapshot.candidate_id
        && validated.plan_hash == snapshot.plan_hash
        && validated.target == snapshot.target
    {
        return true;
    }
    if let Some(selected) = context.selected_candidate.as_ref() {
        return selected.candidate_id == snapshot.candidate_id
            && selected.plan_hash == snapshot.plan_hash
            && selected.target == snapshot.target;
    }
    false
}

fn preview_invalid_reason(state: &CoreState) -> &'static str {
    let Some(snapshot) = state.preview_snapshot.as_ref() else {
        return "MissingPreviewSnapshot";
    };
    let context = &state.session_context;
    if context.selected_candidate.is_none() && context.validated_plan.is_none() {
        return "MissingSelectedCandidate";
    }
    if let Some(selected) = context.selected_candidate.as_ref() {
        if selected.candidate_id != snapshot.candidate_id {
            return "CandidateIdMismatch";
        }
        if selected.plan_hash != snapshot.plan_hash {
            return "PlanHashMismatch";
        }
    }
    if let Some(validated) = context.validated_plan.as_ref() {
        if validated.candidate_id != snapshot.candidate_id {
            return "CandidateIdMismatch";
        }
        if validated.plan_hash != snapshot.plan_hash {
            return "PlanHashMismatch";
        }
    }
    "InvalidPreviewSnapshot"
}

fn next_actions_for_pipeline_state(state: &PipelineState) -> Vec<String> {
    match state {
        PipelineState::Previewed => vec!["y".to_string(), "n".to_string(), "cancel".to_string()],
        PipelineState::Proposed => vec!["/proposals".to_string(), "select <n>".to_string()],
        _ => Vec::new(),
    }
}

fn current_timestamp_secs_core() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

impl From<&CoreState> for ApplyGuardInput {
    fn from(state: &CoreState) -> Self {
        Self {
            selected_candidate: state
                .session_context
                .selected_candidate
                .clone()
                .map(SelectedCandidate::from),
            validated_plan: state
                .session_context
                .validated_plan
                .clone()
                .map(ValidatedPlan::from),
            preview_snapshot: state
                .preview_snapshot
                .clone()
                .map(ApplyGuardPreviewSnapshot::from),
            pipeline_state: state.status.clone(),
        }
    }
}

pub fn apply_guard_decide(input: ApplyGuardInput) -> ApplyGuardDecision {
    let Some(validated) = input.validated_plan else {
        return ApplyGuardDecision::Reject {
            reason: ApplyGuardRejectReason::MissingValidatedPlan,
        };
    };
    let Some(selected) = input.selected_candidate else {
        return ApplyGuardDecision::Reject {
            reason: ApplyGuardRejectReason::MissingSelectedCandidate,
        };
    };
    if input.pipeline_state != PipelineState::Previewed {
        return ApplyGuardDecision::Reject {
            reason: ApplyGuardRejectReason::PreviewNotActive,
        };
    }
    let Some(preview) = input.preview_snapshot else {
        return ApplyGuardDecision::Reject {
            reason: ApplyGuardRejectReason::MissingPreviewSnapshot,
        };
    };
    if selected.target == Target::WorkspaceRoot || validated.target == Target::WorkspaceRoot {
        return ApplyGuardDecision::Reject {
            reason: ApplyGuardRejectReason::WorkspaceRootApplyForbidden,
        };
    }
    if selected.candidate_id != validated.candidate_id {
        return ApplyGuardDecision::Reject {
            reason: ApplyGuardRejectReason::CandidateMismatch,
        };
    }
    if selected.plan_hash != validated.plan_hash {
        return ApplyGuardDecision::Reject {
            reason: ApplyGuardRejectReason::PlanHashMismatch,
        };
    }
    if selected.target != validated.target {
        return ApplyGuardDecision::Reject {
            reason: ApplyGuardRejectReason::TargetMismatch,
        };
    }
    if selected.candidate_id != preview.candidate_id
        || validated.candidate_id != preview.candidate_id
    {
        return ApplyGuardDecision::Reject {
            reason: ApplyGuardRejectReason::CandidateMismatch,
        };
    }
    if selected.plan_hash != preview.plan_hash || validated.plan_hash != preview.plan_hash {
        return ApplyGuardDecision::Reject {
            reason: ApplyGuardRejectReason::PlanHashMismatch,
        };
    }
    if selected.target != preview.target || validated.target != preview.target {
        return ApplyGuardDecision::Reject {
            reason: ApplyGuardRejectReason::TargetMismatch,
        };
    }
    if !validated.approved || !validated.apply_allowed || validated.timestamp < selected.timestamp {
        return ApplyGuardDecision::Reject {
            reason: ApplyGuardRejectReason::StaleValidatedPlan,
        };
    }
    ApplyGuardDecision::Allow {
        candidate_id: validated.candidate_id,
        target: validated.target,
        validated_plan_hash: validated.plan_hash,
    }
}

fn parse_preview_confirmation(input: &str) -> PreviewConfirmation {
    match input.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" => PreviewConfirmation::Confirm,
        "n" | "no" => PreviewConfirmation::Reject,
        "cancel" => PreviewConfirmation::Cancel,
        _ => PreviewConfirmation::Reconfirm,
    }
}

fn confirmation_token_check_trace(input: &str, action: PreviewConfirmation) -> String {
    let (result, reason) = match action {
        PreviewConfirmation::Confirm => ("Confirm", "ExactToken"),
        PreviewConfirmation::Reject => ("Reject", "ExactToken"),
        PreviewConfirmation::Cancel => ("Cancel", "ExactToken"),
        PreviewConfirmation::Reconfirm => ("NotConfirmation", "NotExactToken"),
    };
    format!(
        "[IR-TRACE][CONFIRMATION_TOKEN_CHECK] result={result} reason={reason} input={:?}",
        input.trim()
    )
}

fn should_route_preview_input_to_language(input: &str) -> bool {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed == "適用する？" {
        return false;
    }
    if is_plan_validation_intent(trimmed) {
        return true;
    }
    !matches!(
        crate::nl::language_core_ir_adapter::classify_language_core_intent(trimmed),
        crate::nl::language_core_ir_adapter::LanguageCoreIntent::Unknown { .. }
    )
}

fn ir_plan_json(plan: &strategy_engine::CodeIrProgram) -> serde_json::Value {
    let steps = execution_steps(plan);
    json!({
        "plan_id": checksum_bytes(format!("{plan:?}").as_bytes()),
        "steps_count": steps.len(),
        "steps": steps,
        "unresolved": plan.build_plan.build_commands.is_empty()
            && plan.run_plan.run_commands.is_empty()
            && plan.test_plan.test_commands.is_empty(),
    })
}

fn execution_plan_json(plan: &strategy_engine::CodeIrProgram) -> serde_json::Value {
    json!({
        "language": format!("{:?}", plan.language),
        "framework": plan.framework,
        "resolved_target": plan.project_root.display().to_string(),
        "constraints": {
            "manifest": plan.dependency_plan.manifest_file,
            "dependencies": plan.dependency_plan.dependencies.len(),
        },
        "execution_steps": execution_steps(plan),
    })
}

fn execution_steps(plan: &strategy_engine::CodeIrProgram) -> Vec<serde_json::Value> {
    let mut steps = Vec::new();
    for command in &plan.dependency_plan.install_commands {
        steps.push(json!({
            "op": "InstallDependency",
            "target": plan.dependency_plan.manifest_file,
            "command": command,
        }));
    }
    for command in &plan.build_plan.build_commands {
        steps.push(json!({
            "op": "Build",
            "target": plan.project_root.display().to_string(),
            "command": command,
        }));
    }
    for command in &plan.run_plan.run_commands {
        steps.push(json!({
            "op": "Run",
            "target": plan.project_root.display().to_string(),
            "command": command,
        }));
    }
    for command in &plan.test_plan.test_commands {
        steps.push(json!({
            "op": "Test",
            "target": plan.project_root.display().to_string(),
            "command": command,
        }));
    }
    steps
}

fn candidates_json_from_strategy(output: &StrategyOutput) -> serde_json::Value {
    let candidates = output
        .strategy_trace
        .attempts
        .iter()
        .map(|attempt| {
            json!({
                "kind": attempt.strategy_kind.to_string(),
                "score": if attempt.success { 1.0 } else { 0.0 },
                "expected_gain": if attempt.success { 1.0 } else { 0.0 },
                "risk": if attempt.success { 0.0 } else { 1.0 },
                "attempt": attempt.attempt_index,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "count": candidates.len(),
        "candidates": candidates,
        "empty": candidates.is_empty(),
    })
}

fn selected_json_from_strategy(output: &StrategyOutput) -> serde_json::Value {
    let selected = output.strategy_trace.attempts.last();
    json!({
        "selected_strategy": selected
            .map(|attempt| attempt.strategy_kind.to_string())
            .unwrap_or_else(|| "none".to_string()),
        "selection_reason": if output.success {
            "successful attempt"
        } else {
            "no successful attempt"
        },
        "score": selected.map(|attempt| if attempt.success { 1.0 } else { 0.0 }),
    })
}

fn execution_result_json(output: &StrategyOutput) -> serde_json::Value {
    let attempts = output
        .strategy_trace
        .attempts
        .iter()
        .map(|attempt| {
            json!({
                "attempt": attempt.attempt_index,
                "status": if attempt.success { "success" } else { "failure" },
                "stdout": attempt.stdout,
                "stderr": attempt.stderr,
                "effects": {
                    "strategy": attempt.strategy_kind.to_string(),
                    "plan_checksum": attempt.plan_checksum.to_string(),
                },
                "error": attempt
                    .failure_context
                    .as_ref()
                    .map(|failure| format!("{:?}", failure.error)),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "status": if output.success { "success" } else { "failure" },
        "outputs": attempts,
        "effects": {
            "selected_plan": checksum_bytes(format!("{:?}", output.selected_plan).as_bytes()),
        },
        "error": if output.success {
            serde_json::Value::Null
        } else {
            json!(output.strategy_trace.final_outcome.to_string())
        },
    })
}

fn trace_unsupported_operation(step_id: &str, op: &str, target: Option<&str>, reason: &str) {
    let data = ir_trace_json(
        "ERROR",
        json!({
            "kind": "Unsupported operation",
            "step_id": step_id,
            "op": op,
            "target": target,
            "reason": reason,
        }),
    );
    trace_ir!(TraceLevel::Error, "ERROR", data);
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingFile {
    path: String,
    content: String,
    content_checksum: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AppliedFile {
    path: String,
    target: PathBuf,
    backup: Option<Vec<u8>>,
    before_checksum: Option<String>,
    after_checksum: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct GitSnapshot {
    modified: Vec<String>,
    staged: Vec<String>,
    head: String,
}

impl GitSnapshot {
    fn from_applied(applied: &[AppliedFile]) -> Self {
        Self {
            modified: applied.iter().map(|file| file.path.clone()).collect(),
            staged: Vec::new(),
            head: String::new(),
        }
    }
}

// ── Select / candidate conversion ────────────────────────────────────────────

/// Internal plan generated from a selected `ExecutionPlanCandidate`.
/// Phase 1C.5 §6.
struct SelectionPlan {
    /// Human-readable step labels derived from the candidate's ops.
    steps: Vec<String>,
}

/// Convert a candidate to an internal `SelectionPlan`.
///
/// Phase 1C.5 §6.1–§6.3
/// Returns `Err` for empty steps or unresolved (empty) target files.
fn candidate_to_execution_plan(
    candidate: &ExecutionPlanCandidate,
) -> Result<SelectionPlan, String> {
    if candidate.steps.is_empty() {
        return Err("candidate has no steps".to_string());
    }
    if let Some(ref target) = candidate.target
        && target.file.is_empty()
    {
        return Err("unresolved target file".to_string());
    }
    let steps: Vec<String> = candidate.steps.iter().map(|op| op.label()).collect();
    Ok(SelectionPlan { steps })
}

/// Derive a new `CoreState` by applying `events` on top of `previous`.
///
/// Used by `push_and_attach_core_state` to build the snapshot that gets
/// stored in `History`.  The `design` parameter overrides `previous.design`
/// when the command produced a fresh design document.
fn core_state_from_events(
    previous: &CoreState,
    events: &[CoreEvent],
    design: Option<&DesignDocument>,
) -> CoreState {
    let mut next = previous.clone();
    // version will be set by History::push; zero it here so push assigns correctly.
    next.version = 0;

    for event in events {
        match event {
            CoreEvent::Pipeline { state: label } => {
                if let Some(ps) = pipeline_state_from_label_core(label) {
                    next.status = ps;
                }
            }
            CoreEvent::Proposal { candidates } => {
                next.proposals = candidates.clone();
            }
            CoreEvent::Plan { steps } => {
                next.current_plan = Some(CorePlan {
                    summary: steps.first().cloned().unwrap_or_default(),
                    steps: steps.clone(),
                });
            }
            CoreEvent::Diff { file, changes } => {
                next.last_diff = Some(Diff {
                    file: file.clone(),
                    changes: changes.clone(),
                });
            }
            _ => {}
        }
    }

    if let Some(doc) = design {
        next.design = Some(doc.clone());
    }

    next
}

fn pipeline_state_from_label_core(label: &str) -> Option<PipelineState> {
    match label {
        "Idle" => Some(PipelineState::Idle),
        "Proposed" => Some(PipelineState::Proposed),
        "Planned" => Some(PipelineState::Planned),
        "Previewed" => Some(PipelineState::Previewed),
        "Applied" => Some(PipelineState::Applied),
        "Staged" => Some(PipelineState::Staged),
        "Committed" => Some(PipelineState::Committed),
        _ => None,
    }
}

fn is_exploration_state(state: &PipelineState) -> bool {
    matches!(
        state,
        PipelineState::Proposed | PipelineState::Planned | PipelineState::Previewed
    )
}

fn replay_validation_failure_response(
    current: CoreState,
    failure: ReplayValidationFailure,
    target_version: Option<u64>,
    replay_distance: Option<usize>,
    replay_limit: Option<usize>,
) -> CoreResponse {
    let message = match failure {
        ReplayValidationFailure::InvalidTarget => "Replay target does not exist",
        ReplayValidationFailure::ReplayLimitExceeded => "Replay limit exceeded",
        ReplayValidationFailure::ParseFailure => "Invalid replay target",
    };
    let target = target_version
        .map(|version| version.to_string())
        .unwrap_or_else(|| "none".to_string());

    let mut events = vec![CoreEvent::Debug {
        message: format!("[IR-TRACE][REPLAY_TARGET_VALIDATE] target_version={target}"),
    }];
    if let Some(distance) = replay_distance {
        events.push(CoreEvent::Debug {
            message: format!(
                "[IR-TRACE][REPLAY_DISTANCE_VALIDATE] target_version={target} distance={distance} limit={}",
                replay_limit.unwrap_or_default()
            ),
        });
    }
    events.push(CoreEvent::Debug {
        message: format!(
            "[IR-TRACE][REPLAY_TARGET_REJECTED] target_version={target} reason={failure:?}"
        ),
    });
    events.push(CoreEvent::Error {
        message: message.to_string(),
    });

    CoreResponse {
        events,
        status: ExecutionStatus::Failed,
        design: None,
        core_state: Some(current),
    }
}

fn error_response(kind: &str, message: &str, id: u64) -> CoreResponse {
    if observability_enabled() {
        println!(
            "[ERROR][id={}] kind=\"{}\" message=\"{}\"",
            id, kind, message
        );
    }
    let candidates = recovery_candidates(message);
    CoreResponse {
        events: vec![
            CoreEvent::Error {
                message: format!("{kind}: {message}"),
            },
            CoreEvent::ErrorRecovery { candidates },
            CoreEvent::Next {
                actions: vec![
                    "reselect".to_string(),
                    "undo".to_string(),
                    "retry".to_string(),
                ],
            },
        ],
        status: ExecutionStatus::Failed,
        design: None,
        core_state: None,
    }
}

fn target_context_response(target: PathBuf, _id: u64) -> CoreResponse {
    CoreResponse {
        events: vec![
            CoreEvent::Result {
                message: format!("[TARGET] context set: {}", target.display()),
            },
            CoreEvent::Pipeline {
                state: PipelineState::Idle.label().to_string(),
            },
        ],
        status: ExecutionStatus::Idle,
        design: None,
        core_state: None,
    }
}

fn append_error_with_recovery(events: &mut Vec<CoreEvent>, message: &str) {
    events.push(CoreEvent::Error {
        message: message.to_string(),
    });
    events.push(CoreEvent::ErrorRecovery {
        candidates: recovery_candidates(message),
    });
    events.push(CoreEvent::Next {
        actions: vec![
            "reselect".to_string(),
            "undo".to_string(),
            "retry".to_string(),
        ],
    });
}

fn recovery_candidates(message: &str) -> Vec<ExecutionPlanCandidate> {
    let intent = Intent::new(message.to_string());
    let limits = Limits::default();
    let mut candidates = generate_candidates_from_intent_with_limits(&intent, limits);
    if candidates.is_empty() {
        candidates.push(ExecutionPlanCandidate::from_ops(
            1,
            "Retry with more specific target",
            vec![strategy_engine::ExecutionOp::RuntimePhase(
                "clarify target file or symbol".to_string(),
            )],
            None,
        ));
    }
    candidates.truncate(limits.max_candidates);
    candidates
}

fn preview_lines(files: &[PendingFile]) -> Vec<String> {
    files
        .iter()
        .flat_map(|file| {
            vec![
                format!("--- {}", file.path),
                format!("+++ {}", file.path),
                format!("+{} bytes", file.content.len()),
            ]
        })
        .collect()
}

fn diff_events_from_pending(files: &[PendingFile]) -> Vec<CoreEvent> {
    files
        .iter()
        .map(|file| CoreEvent::Diff {
            file: file.path.clone(),
            changes: unified_chunks_for_generated_file(&file.content),
        })
        .collect()
}

fn unified_chunks_for_generated_file(content: &str) -> Vec<DiffChunk> {
    let mut chunks = vec![DiffChunk {
        old_line: Some(1),
        new_line: Some(1),
        old: Some("@@ -1,0 +1,3 @@".to_string()),
        new: None,
    }];
    chunks.extend(
        content
            .lines()
            .take(12)
            .enumerate()
            .map(|(idx, line)| DiffChunk {
                old_line: None,
                new_line: Some(idx + 1),
                old: None,
                new: Some(line.to_string()),
            }),
    );
    if chunks.len() == 1 {
        chunks.push(DiffChunk {
            old_line: None,
            new_line: Some(1),
            old: None,
            new: Some("(empty generated file)".to_string()),
        });
    }
    chunks
}

fn design_summary(design: &DesignDocument) -> String {
    design
        .reason_units
        .first()
        .map(|unit| unit.summary.clone())
        .unwrap_or_else(|| format!("{} updated", design.structure.module))
}

fn design_diff(previous: &DesignDocument, next: &DesignDocument) -> Vec<String> {
    let previous_lines = previous
        .rendered
        .iter()
        .collect::<std::collections::HashSet<_>>();
    let next_lines = next
        .rendered
        .iter()
        .collect::<std::collections::HashSet<_>>();
    let mut changes = Vec::new();
    for line in &next.rendered {
        if !previous_lines.contains(line) && !line.is_empty() {
            changes.push(format!("+ {line}"));
        }
    }
    for line in &previous.rendered {
        if !next_lines.contains(line) && !line.is_empty() {
            changes.push(format!("- {line}"));
        }
    }
    if changes.is_empty() {
        changes.push("no design changes".to_string());
    }
    changes
}

fn design_score(design: &DesignDocument) -> f64 {
    let reason_score = (design.reason_units.len() as f64 / 8.0).min(0.5);
    let structure_score = if design.structure.functions.is_empty() {
        0.0
    } else {
        0.2
    };
    let constraint_score = (design.constraints.len() as f64 / 4.0).min(0.3);
    reason_score + structure_score + constraint_score
}

fn resolve_repo_file(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.is_absolute() {
        return Err("absolute paths are rejected".to_string());
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("parent directory paths are rejected".to_string());
    }
    Ok(root.join(path))
}

fn apply_intent_target(intent: &Intent) -> Option<PathBuf> {
    intent
        .file
        .clone()
        .or_else(|| intent.target.clone())
        .map(PathBuf::from)
}

fn pending_change_set(root: &Path, pending: &[PendingFile]) -> Result<CodeChangeSet, String> {
    let mut changes = Vec::new();
    for file in pending {
        let target = resolve_repo_file(root, &file.path)?;
        let original = fs::read_to_string(&target).unwrap_or_default();
        let change_type = if target.exists() {
            ChangeType::ModifyFile
        } else {
            ChangeType::CreateFile
        };
        let end_line = original.lines().count().max(1);
        changes.push(CodeChange {
            file_path: file.path.clone(),
            change_type,
            hunks: vec![DiffHunk {
                start_line: 1,
                end_line,
                replacement: file.content.clone(),
            }],
        });
    }
    let summary = summarize_core_changes(&changes);
    let canonical_target = (changes.len() == 1).then(|| PathBuf::from(&changes[0].file_path));
    Ok(CodeChangeSet {
        patches: Vec::new(),
        changes,
        summary,
        canonical_target,
    })
}

fn summarize_core_changes(changes: &[CodeChange]) -> ChangeSummary {
    changes
        .iter()
        .fold(ChangeSummary::default(), |mut summary, change| {
            summary.total_changes += 1;
            match change.change_type {
                ChangeType::CreateFile => summary.create_files += 1,
                ChangeType::ModifyFile => summary.modify_files += 1,
                ChangeType::MoveFile => summary.move_files += 1,
            }
            summary
        })
}

fn planned_applied_files(root: &Path, pending: &[PendingFile]) -> Result<Vec<AppliedFile>, String> {
    pending
        .iter()
        .map(|file| {
            let target = resolve_repo_file(root, &file.path)?;
            let backup = fs::read(&target).ok();
            let before_checksum = backup.as_ref().map(|content| checksum_bytes(content));
            Ok(AppliedFile {
                path: file.path.clone(),
                target,
                backup,
                before_checksum,
                after_checksum: file.content_checksum.clone(),
            })
        })
        .collect()
}

fn verify_applied_files(
    pending: &[PendingFile],
    planned: Vec<AppliedFile>,
) -> Result<Vec<AppliedFile>, String> {
    let expected = pending
        .iter()
        .map(|file| (file.path.as_str(), file.content_checksum.as_str()))
        .collect::<std::collections::BTreeMap<_, _>>();
    for file in &planned {
        let after = fs::read(&file.target)
            .map_err(|err| format!("verify failed for {}: {err}", file.path))?;
        let actual = checksum_bytes(&after);
        let Some(expected_hash) = expected.get(file.path.as_str()) else {
            return Err(format!("missing expected hash for {}", file.path));
        };
        if actual != *expected_hash {
            restore_applied(&planned);
            return Err(format!(
                "{} expected={} actual={}",
                file.path, expected_hash, actual
            ));
        }
    }
    Ok(planned)
}

fn validate_git_add_path(path: &str) -> Result<(), String> {
    crate::git::commands::validate_scoped_add_path(path)?;
    resolve_repo_file(Path::new("."), path).map(|_| ())
}

fn normalize_git_output(output: &str) -> String {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        "(no output)".to_string()
    } else {
        trimmed.to_string()
    }
}

fn is_forbidden_command(lower: &str) -> bool {
    matches!(
        crate::runtime::execution_governance::classify_command(lower),
        crate::runtime::execution_governance::CommandType::Forbidden
            | crate::runtime::execution_governance::CommandType::Dangerous
    ) && (lower.starts_with("git ")
        || lower.starts_with("gh ")
        || lower.starts_with("rm ")
        || lower.starts_with("sudo ")
        || lower.starts_with("shutdown")
        || lower.starts_with("reboot"))
}

fn checksum_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn checksum_u64(bytes: &[u8]) -> u64 {
    let digest = Sha256::digest(bytes);
    u64::from_be_bytes(
        digest[..8]
            .try_into()
            .expect("sha256 digest provides at least eight bytes"),
    )
}

fn create_safe_apply_transaction(
    candidate_id: usize,
    validated_plan_hash: u64,
    target: Target,
    target_path: PathBuf,
    original_contents: String,
    planned_diff_checksum: u64,
) -> SafeApplyTransaction {
    let pre_apply_checksum = checksum_u64(original_contents.as_bytes());
    SafeApplyTransaction {
        transaction_id: next_safe_apply_transaction_id(),
        candidate_id,
        validated_plan_hash,
        target,
        target_path: target_path.clone(),
        pre_apply_checksum,
        planned_diff_checksum,
        rollback_snapshot: RollbackSnapshot {
            target_path,
            original_contents,
            original_checksum: pre_apply_checksum,
        },
    }
}

fn safe_apply_preview_file(
    root: &Path,
    relative_target: &str,
    candidate_id: usize,
) -> Result<PendingFile, String> {
    let target = resolve_repo_file(root, relative_target)?;
    let original = fs::read_to_string(&target)
        .map_err(|err| format!("cannot read selected target {relative_target}: {err}"))?;
    let marker = match target.extension().and_then(|ext| ext.to_str()) {
        Some("rs") => format!("// DBM safe apply candidate {candidate_id}\n"),
        Some("json") => {
            return Err("json safe apply preview is not supported in v1.0".to_string());
        }
        _ => format!("# DBM safe apply candidate {candidate_id}\n"),
    };
    let separator = if original.ends_with('\n') { "" } else { "\n" };
    let content = format!("{original}{separator}{marker}");
    Ok(PendingFile {
        path: relative_target.to_string(),
        content_checksum: checksum_bytes(content.as_bytes()),
        content,
    })
}

fn rollback_safe_apply(
    transaction: &SafeApplyTransaction,
    reason: ApplyFailureReason,
    events: &mut Vec<CoreEvent>,
) -> ApplyResult {
    events.push(CoreEvent::Debug {
        message: format!("[IR-TRACE][SAFE_APPLY_ROLLBACK] reason={reason}"),
    });
    if fs::write(
        &transaction.rollback_snapshot.target_path,
        &transaction.rollback_snapshot.original_contents,
    )
    .is_err()
    {
        return ApplyResult::RolledBack {
            transaction_id: transaction.transaction_id,
            reason: ApplyFailureReason::RollbackFailed,
        };
    }
    ApplyResult::RolledBack {
        transaction_id: transaction.transaction_id,
        reason,
    }
}

fn post_apply_validation(target_path: &Path, contents: &str) -> bool {
    target_path.is_file() && !contents.contains("DBM_POST_VALIDATION_FAIL")
}

fn restore_applied(files: &[AppliedFile]) {
    for file in files.iter().rev() {
        match &file.backup {
            Some(content) => {
                let _ = fs::write(&file.target, content);
            }
            None => {
                let _ = fs::remove_file(&file.target);
            }
        }
    }
}

fn run_git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed to run git: {err}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn sync_pipeline_with_git(root: &Path) -> Result<GitSnapshot, String> {
    Ok(GitSnapshot {
        modified: git_lines(root, &["diff", "--name-only"])?,
        staged: git_lines(root, &["diff", "--cached", "--name-only"])?,
        head: run_git(root, &["rev-parse", "--short", "HEAD"])
            .map(|head| head.trim().to_string())
            .unwrap_or_default(),
    })
}

fn git_lines(root: &Path, args: &[&str]) -> Result<Vec<String>, String> {
    Ok(run_git(root, args)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use strategy_engine::{ExecutionOp, ExecutionPlanCandidate};

    fn request(input: &str) -> CoreRequest {
        CoreRequest::new(input.to_string())
    }

    fn event_text(events: &[CoreEvent]) -> String {
        format!("{events:?}")
    }

    struct SafeApplyFixture {
        _tempdir: tempfile::TempDir,
        root: PathBuf,
        target: PathBuf,
    }

    impl SafeApplyFixture {
        fn new() -> Self {
            let tempdir = tempfile::tempdir().expect("tempdir");
            let root = tempdir.path().to_path_buf();
            let src = root.join("src");
            std::fs::create_dir_all(&src).expect("src dir");
            let target = root.join("Cargo.toml");
            std::fs::write(&target, "[package]\nname = \"fixture\"\n").expect("write target");
            std::fs::write(src.join("lib.rs"), "pub fn fixture() {}\n").expect("write lib");
            Self {
                _tempdir: tempdir,
                root,
                target,
            }
        }

        fn before_contents(&self) -> String {
            std::fs::read_to_string(&self.target).expect("read fixture target")
        }

        fn planned_contents(&self) -> String {
            format!("{}# DBM safe apply candidate 1\n", self.before_contents())
        }

        fn safe_apply_state(&self, planned_content: &str) -> (RuntimeCoreBridge, CoreState) {
            safe_apply_state("Cargo.toml", planned_content)
        }

        fn with_current_dir<T>(&self, run: impl FnOnce() -> T) -> T {
            with_current_dir(&self.root, run)
        }
    }

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("apps dir")
            .parent()
            .expect("repo root")
            .to_path_buf()
    }

    fn init_git_repo(root: &Path) {
        let _guard = crate::test_support::git_guard_lock();
        std::process::Command::new("git")
            .args(["init", "-b", "feature/test"])
            .current_dir(root)
            .output()
            .expect("git init");
        std::process::Command::new("git")
            .args(["config", "user.email", "dbm@example.com"])
            .current_dir(root)
            .output()
            .expect("git email");
        std::process::Command::new("git")
            .args(["config", "user.name", "DBM"])
            .current_dir(root)
            .output()
            .expect("git name");
        std::fs::write(root.join("tracked.txt"), "initial\n").expect("tracked");
        std::process::Command::new("git")
            .args(["add", "tracked.txt"])
            .current_dir(root)
            .output()
            .expect("git add");
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(root)
            .output()
            .expect("git commit");
    }

    use crate::test_support::with_current_dir;

    fn request_with_state(
        core: &RuntimeCoreBridge,
        input: &str,
        state: PipelineState,
        proposals: Option<Vec<ExecutionPlanCandidate>>,
    ) -> CoreRequest {
        let mut hist = core.history.lock().unwrap();
        let mut s = hist.current().clone();
        s.status = state;
        s.proposals = proposals.unwrap_or_default();
        hist.push(s);
        drop(hist);
        CoreRequest::new(input.to_string())
    }

    fn set_preview_state(core: &RuntimeCoreBridge) {
        let mut hist = core.history.lock().unwrap();
        let mut s = hist.current().clone();
        s.status = PipelineState::Previewed;
        s.session_context.selected_candidate = Some(
            crate::nl::context_aware_plan_target_resolver::SelectedCandidateContext {
                candidate_id: 1,
                target: NarrowTarget::File("Cargo.toml".to_string()),
                plan_hash: 42,
                timestamp: 1,
            },
        );
        s.preview_snapshot = Some(PreviewSnapshot {
            candidate_id: 1,
            target: NarrowTarget::File("Cargo.toml".to_string()),
            plan_hash: 42,
            preview_hash: 7,
            created_at: 1,
        });
        hist.push(s);
    }

    fn set_preview_state_with_context(core: &RuntimeCoreBridge) {
        let mut hist = core.history.lock().unwrap();
        let mut s = hist.current().clone();
        s.status = PipelineState::Previewed;
        s.session_context.selected_candidate = Some(
            crate::nl::context_aware_plan_target_resolver::SelectedCandidateContext {
                candidate_id: 1,
                target: NarrowTarget::File("Cargo.toml".to_string()),
                plan_hash: 42,
                timestamp: 1,
            },
        );
        s.session_context.validated_plan = Some(
            crate::nl::context_aware_plan_target_resolver::ValidatedPlanContext {
                plan_hash: 42,
                candidate_id: 1,
                target: NarrowTarget::File("Cargo.toml".to_string()),
                approved: true,
                apply_allowed: true,
                timestamp: 1,
            },
        );
        s.preview_snapshot = Some(PreviewSnapshot {
            candidate_id: 1,
            target: NarrowTarget::File("Cargo.toml".to_string()),
            plan_hash: 42,
            preview_hash: 7,
            created_at: 1,
        });
        hist.push(s);
    }

    fn apply_guard_input() -> ApplyGuardInput {
        ApplyGuardInput {
            selected_candidate: Some(SelectedCandidate {
                candidate_id: 1,
                target: Target::File("Cargo.toml".to_string()),
                plan_hash: 42,
                timestamp: 1,
            }),
            validated_plan: Some(ValidatedPlan {
                plan_hash: 42,
                candidate_id: 1,
                target: Target::File("Cargo.toml".to_string()),
                approved: true,
                apply_allowed: true,
                timestamp: 2,
            }),
            preview_snapshot: Some(ApplyGuardPreviewSnapshot {
                candidate_id: 1,
                target: Target::File("Cargo.toml".to_string()),
                plan_hash: 42,
            }),
            pipeline_state: PipelineState::Previewed,
        }
    }

    fn assert_apply_guard_rejects(input: ApplyGuardInput, expected: ApplyGuardRejectReason) {
        assert_eq!(
            apply_guard_decide(input),
            ApplyGuardDecision::Reject { reason: expected }
        );
    }

    fn assert_guard_rejected_without_apply(
        response: &CoreResponse,
        expected_reason: &str,
        expected_file_contents: (&Path, &str),
    ) {
        let state = response.core_state.as_ref().expect("state");
        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains(&format!("[IR-TRACE][APPLY_GUARD] rejected=true reason={expected_reason}")))));
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][CONTEXT_CLEAR] reason=ApplyGuardReject clears preview/selection"))));
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][PIPELINE_DERIVE] state=Idle reason=ApplyRejected"))));
        assert!(!response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][SAFE_APPLY_BEGIN]"))));
        assert!(!response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][SAFE_APPLY_SUCCESS]"))));
        assert_eq!(
            std::fs::read_to_string(expected_file_contents.0).expect("read after"),
            expected_file_contents.1
        );
        assert_eq!(state.status, PipelineState::Idle);
        assert!(state.preview_snapshot.is_none());
        assert!(state.session_context.selected_candidate.is_none());
        assert!(state.session_context.validated_plan.is_none());
    }

    fn safe_apply_state(relative: &str, planned_content: &str) -> (RuntimeCoreBridge, CoreState) {
        let core = RuntimeCoreBridge::with_defaults();
        *core.pending_files.lock().expect("pending lock") = vec![PendingFile {
            path: relative.to_string(),
            content: planned_content.to_string(),
            content_checksum: checksum_bytes(planned_content.as_bytes()),
        }];
        let preview_hash = checksum_u64(
            preview_lines(&core.pending_files.lock().expect("pending lock"))
                .join("\n")
                .as_bytes(),
        );
        let mut state = CoreState {
            status: PipelineState::Previewed,
            ..CoreState::default()
        };
        state.session_context.selected_candidate = Some(SelectedCandidateContext {
            candidate_id: 1,
            target: NarrowTarget::File(relative.to_string()),
            plan_hash: 42,
            timestamp: 1,
        });
        state.session_context.validated_plan = Some(ValidatedPlanContext {
            plan_hash: 42,
            candidate_id: 1,
            target: NarrowTarget::File(relative.to_string()),
            approved: true,
            apply_allowed: true,
            timestamp: 2,
        });
        state.preview_snapshot = Some(PreviewSnapshot {
            candidate_id: 1,
            target: NarrowTarget::File(relative.to_string()),
            plan_hash: 42,
            preview_hash,
            created_at: 1,
        });
        (core, state)
    }

    fn state_with_plan_context() -> CoreState {
        let mut state = CoreState::default();
        state.session_context.previous_analysis_context = Some(PreviousAnalysisContext {
            action: IrAction::AnalyzeProject,
            target: IrTarget::WorkspaceRoot,
            mode: crate::nl::language_core_ir_adapter::ExecutionMode::ReadOnly,
            status: ExecutionStatus::Executed,
            summary_hash: 1,
            timestamp: 1,
        });
        state.session_context.previous_plan_context = Some(
            crate::nl::context_aware_plan_target_resolver::PreviousPlanContext {
                target: IrTarget::WorkspaceRoot,
                mode: crate::nl::language_core_ir_adapter::ExecutionMode::PlanOnly,
                candidate_count: 1,
                candidates: vec![ChangePlanCandidate {
                    candidate_id: 1,
                    title: "test".to_string(),
                    target: NarrowTarget::File("Cargo.toml".to_string()),
                    proposed_change: "test".to_string(),
                    rationale: "test".to_string(),
                    risk_level: crate::runtime::autonomous_control::RiskLevel::Low,
                    requires_validation: true,
                }],
                plan_hash: 42,
                status: ExecutionStatus::Executed,
                timestamp: 1,
            },
        );
        state.previous_analysis_context = state.session_context.previous_analysis_context.clone();
        state
    }

    fn core_with_limits(limits: Limits) -> RuntimeCoreBridge {
        RuntimeCoreBridge::new_with_limits(
            CoreRuntime::new_with_defaults(
                Arc::new(InMemoryEngine::default()),
                Arc::new(DeterministicBeamSearchEngine::default()),
            ),
            StrategyEngine::default(),
            limits,
        )
    }

    #[test]
    fn preview_confirmation_parser_maps_confirm_reject_cancel_and_reconfirm() {
        assert_eq!(
            parse_preview_confirmation("y"),
            PreviewConfirmation::Confirm
        );
        assert_eq!(
            parse_preview_confirmation("yes"),
            PreviewConfirmation::Confirm
        );
        assert_eq!(parse_preview_confirmation("n"), PreviewConfirmation::Reject);
        assert_eq!(
            parse_preview_confirmation("no"),
            PreviewConfirmation::Reject
        );
        assert_eq!(
            parse_preview_confirmation("cancel"),
            PreviewConfirmation::Cancel
        );
        assert_eq!(
            parse_preview_confirmation(""),
            PreviewConfirmation::Reconfirm
        );
        assert_eq!(
            parse_preview_confirmation("maybe"),
            PreviewConfirmation::Reconfirm
        );
    }

    #[test]
    fn apply_guard_rejects_without_validated_plan() {
        let mut input = apply_guard_input();
        input.validated_plan = None;

        assert_apply_guard_rejects(input, ApplyGuardRejectReason::MissingValidatedPlan);
    }

    #[test]
    fn apply_guard_rejects_without_selected_candidate() {
        let mut input = apply_guard_input();
        input.selected_candidate = None;

        assert_apply_guard_rejects(input, ApplyGuardRejectReason::MissingSelectedCandidate);
    }

    #[test]
    fn apply_guard_rejects_workspace_root_apply() {
        let mut input = apply_guard_input();
        input.selected_candidate.as_mut().expect("selected").target = Target::WorkspaceRoot;
        input.validated_plan.as_mut().expect("validated").target = Target::WorkspaceRoot;

        assert_apply_guard_rejects(input, ApplyGuardRejectReason::WorkspaceRootApplyForbidden);
    }

    #[test]
    fn apply_guard_rejects_candidate_mismatch() {
        let mut input = apply_guard_input();
        input
            .validated_plan
            .as_mut()
            .expect("validated")
            .candidate_id = 2;

        assert_apply_guard_rejects(input, ApplyGuardRejectReason::CandidateMismatch);
    }

    #[test]
    fn apply_guard_rejects_plan_hash_mismatch() {
        let mut input = apply_guard_input();
        input.validated_plan.as_mut().expect("validated").plan_hash = 43;

        assert_apply_guard_rejects(input, ApplyGuardRejectReason::PlanHashMismatch);
    }

    #[test]
    fn apply_guard_rejects_missing_preview_snapshot() {
        let mut input = apply_guard_input();
        input.preview_snapshot = None;

        assert_apply_guard_rejects(input, ApplyGuardRejectReason::MissingPreviewSnapshot);
    }

    #[test]
    fn apply_guard_rejects_preview_plan_hash_mismatch() {
        let mut input = apply_guard_input();
        input.preview_snapshot.as_mut().expect("preview").plan_hash = 43;

        assert_apply_guard_rejects(input, ApplyGuardRejectReason::PlanHashMismatch);
    }

    #[test]
    fn apply_guard_rejects_target_mismatch() {
        let mut input = apply_guard_input();
        input.validated_plan.as_mut().expect("validated").target =
            Target::File("apps/cli/src/core.rs".to_string());

        assert_apply_guard_rejects(input, ApplyGuardRejectReason::TargetMismatch);
    }

    #[test]
    fn apply_guard_rejects_non_previewed_pipeline() {
        let mut input = apply_guard_input();
        input.pipeline_state = PipelineState::Planned;

        assert_apply_guard_rejects(input, ApplyGuardRejectReason::PreviewNotActive);
    }

    #[test]
    fn apply_guard_rejects_stale_validated_plan() {
        let mut input = apply_guard_input();
        input
            .selected_candidate
            .as_mut()
            .expect("selected")
            .timestamp = 3;
        input.validated_plan.as_mut().expect("validated").timestamp = 2;

        assert_apply_guard_rejects(input, ApplyGuardRejectReason::StaleValidatedPlan);
    }

    #[test]
    fn apply_guard_allows_validated_narrow_target() {
        assert_eq!(
            apply_guard_decide(apply_guard_input()),
            ApplyGuardDecision::Allow {
                candidate_id: 1,
                target: Target::File("Cargo.toml".to_string()),
                validated_plan_hash: 42,
            }
        );
    }

    #[test]
    fn safe_apply_creates_transaction() {
        let fixture = SafeApplyFixture::new();
        let before = fixture.before_contents();
        let tx = create_safe_apply_transaction(
            1,
            42,
            Target::File("Cargo.toml".to_string()),
            fixture.target.clone(),
            before,
            7,
        );

        assert_eq!(tx.candidate_id, 1);
        assert_eq!(tx.validated_plan_hash, 42);
        assert_eq!(tx.target, Target::File("Cargo.toml".to_string()));
        assert!(tx.rollback_snapshot.target_path.starts_with(&fixture.root));
        assert_eq!(tx.planned_diff_checksum, 7);
    }

    #[test]
    fn safe_apply_creates_rollback_snapshot() {
        let fixture = SafeApplyFixture::new();
        let before = fixture.before_contents();
        let tx = create_safe_apply_transaction(
            1,
            42,
            Target::File("Cargo.toml".to_string()),
            fixture.target.clone(),
            before.clone(),
            7,
        );

        assert_eq!(tx.rollback_snapshot.original_contents, before);
        assert_eq!(tx.rollback_snapshot.target_path, fixture.target);
        assert!(tx.rollback_snapshot.target_path.starts_with(&fixture.root));
    }

    #[test]
    fn safe_apply_records_pre_checksum() {
        let fixture = SafeApplyFixture::new();
        let before = fixture.before_contents();
        let tx = create_safe_apply_transaction(
            1,
            42,
            Target::File("Cargo.toml".to_string()),
            fixture.target,
            before.clone(),
            7,
        );

        assert_eq!(tx.pre_apply_checksum, checksum_u64(before.as_bytes()));
        assert_eq!(
            tx.pre_apply_checksum,
            tx.rollback_snapshot.original_checksum
        );
    }

    #[test]
    fn safe_apply_requires_apply_guard_allow() {
        let fixture = SafeApplyFixture::new();
        let (core, state) = fixture.safe_apply_state("after\n");
        let mut events = Vec::new();

        let result = core.execute_safe_apply(
            &fixture.root,
            &state,
            &ApplyGuardDecision::Reject {
                reason: ApplyGuardRejectReason::MissingValidatedPlan,
            },
            &mut events,
        );

        assert_eq!(
            result,
            ApplyResult::Rejected {
                reason: ApplyRejectReason::GuardRejected
            }
        );
    }

    #[test]
    fn safe_apply_rejects_without_validated_plan() {
        let fixture = SafeApplyFixture::new();
        let before = fixture.before_contents();
        let (core, mut state) = fixture.safe_apply_state("after\n");
        state.session_context.validated_plan = None;
        let mut events = Vec::new();

        let result = core.execute_safe_apply(
            &fixture.root,
            &state,
            &ApplyGuardDecision::Allow {
                candidate_id: 1,
                target: Target::File("Cargo.toml".to_string()),
                validated_plan_hash: 42,
            },
            &mut events,
        );

        assert_eq!(
            result,
            ApplyResult::Rejected {
                reason: ApplyRejectReason::MissingValidatedPlan
            }
        );
        assert_eq!(
            std::fs::read_to_string(&fixture.target).expect("read"),
            before
        );
    }

    #[test]
    fn safe_apply_rejects_workspace_root() {
        let fixture = SafeApplyFixture::new();
        let (core, state) = fixture.safe_apply_state("after\n");
        let mut events = Vec::new();

        let result = core.execute_safe_apply(
            &fixture.root,
            &state,
            &ApplyGuardDecision::Allow {
                candidate_id: 1,
                target: Target::WorkspaceRoot,
                validated_plan_hash: 42,
            },
            &mut events,
        );

        assert_eq!(
            result,
            ApplyResult::Rejected {
                reason: ApplyRejectReason::WorkspaceRootApplyForbidden
            }
        );
    }

    #[test]
    fn safe_apply_rolls_back_on_checksum_mismatch() {
        let fixture = SafeApplyFixture::new();
        let before = fixture.before_contents();
        let tx = create_safe_apply_transaction(
            1,
            42,
            Target::File("Cargo.toml".to_string()),
            fixture.target.clone(),
            before.clone(),
            7,
        );
        std::fs::write(&fixture.target, "partial\n").expect("partial");
        let mut events = Vec::new();

        let result = rollback_safe_apply(&tx, ApplyFailureReason::ChecksumMismatch, &mut events);

        assert_eq!(
            result,
            ApplyResult::RolledBack {
                transaction_id: tx.transaction_id,
                reason: ApplyFailureReason::ChecksumMismatch
            }
        );
        assert_eq!(
            std::fs::read_to_string(&fixture.target).expect("read"),
            before
        );
        assert!(events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][SAFE_APPLY_ROLLBACK] reason=ChecksumMismatch"))));
    }

    #[test]
    fn safe_apply_rolls_back_on_post_validation_failure() {
        let fixture = SafeApplyFixture::new();
        let before = fixture.before_contents();
        let (core, state) = fixture.safe_apply_state("DBM_POST_VALIDATION_FAIL\n");
        let guard = apply_guard_decide(ApplyGuardInput::from(&state));
        let mut events = Vec::new();

        let result = core.execute_safe_apply(&fixture.root, &state, &guard, &mut events);

        assert!(matches!(
            result,
            ApplyResult::RolledBack {
                reason: ApplyFailureReason::PostValidationFailed,
                ..
            }
        ));
        assert_eq!(
            std::fs::read_to_string(&fixture.target).expect("read"),
            before
        );
        assert!(events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][SAFE_APPLY_ROLLBACK] reason=PostValidationFailed"))));
    }

    #[test]
    fn safe_apply_uses_isolated_fixture_workspace() {
        let fixture = SafeApplyFixture::new();
        let repo_cargo_before =
            std::fs::read_to_string(repo_root().join("Cargo.toml")).expect("repo cargo before");
        let planned = fixture.planned_contents();
        let (core, state) = fixture.safe_apply_state(&planned);
        let guard = apply_guard_decide(ApplyGuardInput::from(&state));
        let mut events = Vec::new();

        let result = core.execute_safe_apply(&fixture.root, &state, &guard, &mut events);

        assert!(matches!(result, ApplyResult::Applied { .. }));
        assert!(fixture.target.starts_with(&fixture.root));
        assert!(
            std::fs::read_to_string(&fixture.target)
                .expect("fixture after")
                .contains("# DBM safe apply candidate 1")
        );
        assert_eq!(
            std::fs::read_to_string(repo_root().join("Cargo.toml")).expect("repo cargo after"),
            repo_cargo_before
        );
        assert!(events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][SAFE_APPLY_BEGIN]") && message.contains("target=File(Cargo.toml)"))));
        assert!(events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][ROLLBACK_SNAPSHOT_CREATED]"))));
        assert!(events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][SAFE_APPLY_SUCCESS]"))));
    }

    #[test]
    fn safe_apply_tests_do_not_modify_repository_worktree() {
        let repo_root = repo_root();
        let repo_cargo = repo_root.join("Cargo.toml");
        let cli_cargo = repo_root.join("apps/cli/Cargo.toml");
        let repo_cargo_before = std::fs::read_to_string(&repo_cargo).expect("repo cargo before");
        let cli_cargo_before = std::fs::read_to_string(&cli_cargo).expect("cli cargo before");
        let fixture = SafeApplyFixture::new();
        let planned = fixture.planned_contents();
        let (core, state) = fixture.safe_apply_state(&planned);
        let guard = apply_guard_decide(ApplyGuardInput::from(&state));
        let mut events = Vec::new();

        let result = core.execute_safe_apply(&fixture.root, &state, &guard, &mut events);

        assert!(matches!(result, ApplyResult::Applied { .. }));
        assert_eq!(
            std::fs::read_to_string(&repo_cargo).expect("repo cargo after"),
            repo_cargo_before
        );
        assert_eq!(
            std::fs::read_to_string(&cli_cargo).expect("cli cargo after"),
            cli_cargo_before
        );
    }

    #[test]
    fn confirmation_exact_y_is_confirm() {
        assert_eq!(
            parse_preview_confirmation("y"),
            PreviewConfirmation::Confirm
        );
    }

    #[test]
    fn confirmation_exact_yes_is_confirm() {
        assert_eq!(
            parse_preview_confirmation(" yes "),
            PreviewConfirmation::Confirm
        );
    }

    #[test]
    fn confirmation_sentence_yes_no_is_not_confirmation() {
        assert_eq!(
            parse_preview_confirmation("この変更案について yes/no の確認ではなく評価してください"),
            PreviewConfirmation::Reconfirm
        );
    }

    #[test]
    fn confirmation_sentence_y_n_is_not_confirmation() {
        assert_eq!(
            parse_preview_confirmation(
                "y や n という文字が含まれていても confirmation として扱わない"
            ),
            PreviewConfirmation::Reconfirm
        );
    }

    #[test]
    fn derive_pipeline_state_previewed_requires_preview_snapshot() {
        let mut state = state_with_plan_context();
        state.session_context.selected_candidate = Some(
            crate::nl::context_aware_plan_target_resolver::SelectedCandidateContext {
                candidate_id: 1,
                target: NarrowTarget::File("Cargo.toml".to_string()),
                plan_hash: 42,
                timestamp: 1,
            },
        );

        assert_ne!(
            derive_pipeline_state_from_context(&state.session_context, None),
            PipelineState::Previewed
        );
    }

    #[test]
    fn derive_pipeline_state_previewed_requires_selected_candidate() {
        let state = state_with_plan_context();
        let snapshot = PreviewSnapshot {
            candidate_id: 1,
            target: NarrowTarget::File("Cargo.toml".to_string()),
            plan_hash: 42,
            preview_hash: 7,
            created_at: 1,
        };

        assert_eq!(
            derive_pipeline_state_from_context(&state.session_context, Some(&snapshot)),
            PipelineState::Proposed
        );
    }

    #[test]
    fn derive_pipeline_state_previous_plan_without_selection_returns_proposed() {
        let state = state_with_plan_context();

        assert_eq!(
            derive_pipeline_state_from_context(&state.session_context, None),
            PipelineState::Proposed
        );
    }

    #[test]
    fn rollback_state_downgrades_previewed_without_selection() {
        let mut restored = state_with_plan_context();
        restored.status = PipelineState::Previewed;
        restored.preview_snapshot = Some(PreviewSnapshot {
            candidate_id: 1,
            target: NarrowTarget::File("Cargo.toml".to_string()),
            plan_hash: 42,
            preview_hash: 7,
            created_at: 1,
        });

        let (restored, check) =
            normalize_restored_state_after_undo(restored, &CoreState::default());

        assert!(!check.valid);
        assert_eq!(restored.status, PipelineState::Proposed);
    }

    #[test]
    fn rollback_state_downgrades_previewed_without_preview_snapshot() {
        let mut restored = state_with_plan_context();
        restored.status = PipelineState::Previewed;
        restored.session_context.selected_candidate = Some(
            crate::nl::context_aware_plan_target_resolver::SelectedCandidateContext {
                candidate_id: 1,
                target: NarrowTarget::File("Cargo.toml".to_string()),
                plan_hash: 42,
                timestamp: 1,
            },
        );

        let (restored, check) =
            normalize_restored_state_after_undo(restored, &CoreState::default());

        assert!(!check.valid);
        assert_eq!(restored.status, PipelineState::Proposed);
    }

    #[test]
    fn preview_empty_input_reconfirms() {
        let core = RuntimeCoreBridge::with_defaults();
        set_preview_state(&core);

        let response = core.execute(request(""));

        assert_eq!(response.status, ExecutionStatus::Idle);
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Result { message } if message == "Please confirm: y / n / cancel")));
        assert_eq!(
            response.core_state.expect("state").status,
            PipelineState::Previewed
        );
    }

    #[test]
    fn preview_unknown_input_reconfirms() {
        let core = RuntimeCoreBridge::with_defaults();
        set_preview_state(&core);

        let response = core.execute(request("maybe"));

        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][PREVIEW_CONFIRMATION] action=Reconfirm"))));
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Result { message } if message == "Please confirm: y / n / cancel")));
        assert_eq!(
            response.core_state.expect("state").status,
            PipelineState::Previewed
        );
    }

    #[test]
    fn preview_uncertain_japanese_apply_input_reconfirms() {
        let core = RuntimeCoreBridge::with_defaults();
        set_preview_state(&core);

        let response = core.execute(request("適用する？"));

        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][PREVIEW_CONFIRMATION] action=Reconfirm"))));
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Result { message } if message == "Please confirm: y / n / cancel")));
        assert_eq!(
            response.core_state.expect("state").status,
            PipelineState::Previewed
        );
    }

    #[test]
    fn preview_unknown_input_does_not_route_to_language_core() {
        let core = RuntimeCoreBridge::with_defaults();
        set_preview_state(&core);

        let response = core.execute(request("abc"));

        assert!(!response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("LANGUAGE_CORE"))));
        assert!(!response.events.iter().any(|event| matches!(event, CoreEvent::Error { message } if message.contains("ClarificationRequired"))));
    }

    #[test]
    fn preview_reject_cancels_without_clarification() {
        let core = RuntimeCoreBridge::with_defaults();
        set_preview_state(&core);

        let response = core.execute(request("n"));
        let state = response.core_state.as_ref().expect("state");

        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][PREVIEW_CONFIRMATION] action=Reject"))));
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Result { message } if message == "Preview cancelled. No files modified.")));
        assert!(!response.events.iter().any(|event| matches!(event, CoreEvent::Error { message } if message.contains("ClarificationRequired"))));
        assert_eq!(state.status, PipelineState::Idle);
        assert!(state.preview_snapshot.is_none());
        assert!(state.session_context.selected_candidate.is_none());
    }

    #[test]
    fn preview_confirm_without_validated_plan_rejects_apply() {
        let core = RuntimeCoreBridge::with_defaults();
        set_preview_state(&core);

        let response = core.execute(request("y"));
        let state = response.core_state.as_ref().expect("state");

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][PREVIEW_CONFIRMATION] action=Confirm"))));
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][APPLY_GUARD] rejected=true reason=MissingValidatedPlan"))));
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Result { message } if message.contains("# Apply Rejected") && message.contains("No files modified."))));
        assert_eq!(state.status, PipelineState::Idle);
        assert!(state.preview_snapshot.is_none());
        assert!(state.session_context.selected_candidate.is_none());
    }

    #[test]
    fn repl_y_without_validated_plan_rejects_apply_guard() {
        let temp = tempfile::tempdir().expect("tempdir");
        let target = temp.path().join("Cargo.toml");
        std::fs::write(&target, "[package]\nname = \"fixture\"\n").expect("write");
        let before = std::fs::read_to_string(&target).expect("read before");
        let core = RuntimeCoreBridge::with_defaults();
        set_preview_state(&core);

        let response = core.execute(request("y"));

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][APPLY_GUARD] rejected=true reason=MissingValidatedPlan"))));
        assert_eq!(
            std::fs::read_to_string(&target).expect("read after"),
            before
        );
    }

    #[test]
    fn repl_stale_validated_plan_rejected() {
        let core = RuntimeCoreBridge::with_defaults();
        set_preview_state_with_context(&core);
        {
            let mut hist = core.history.lock().expect("history lock");
            let mut state = hist.current().clone();
            state
                .session_context
                .selected_candidate
                .as_mut()
                .expect("selected")
                .timestamp = 3;
            state
                .session_context
                .validated_plan
                .as_mut()
                .expect("validated")
                .timestamp = 2;
            hist.replace_current(state);
        }

        let response = core.execute(request("y"));

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][APPLY_GUARD] rejected=true reason=StaleValidatedPlan"))));
    }

    #[test]
    fn safe_apply_rejects_stale_validated_plan_hash() {
        let fixture = SafeApplyFixture::new();
        let before = fixture.before_contents();
        let response = fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            let (fixture_core, mut state) = fixture.safe_apply_state(&fixture.planned_contents());
            state
                .session_context
                .validated_plan
                .as_mut()
                .expect("validated")
                .plan_hash = 43;
            *core.pending_files.lock().expect("pending lock") = fixture_core
                .pending_files
                .lock()
                .expect("pending lock")
                .clone();
            core.history
                .lock()
                .expect("history lock")
                .replace_current(state);
            core.execute(request("y"))
        });

        assert_guard_rejected_without_apply(
            &response,
            "PlanHashMismatch",
            (&fixture.target, &before),
        );
    }

    #[test]
    fn safe_apply_rejects_validated_plan_candidate_mismatch() {
        let fixture = SafeApplyFixture::new();
        let before = fixture.before_contents();
        let response = fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            let (fixture_core, mut state) = fixture.safe_apply_state(&fixture.planned_contents());
            state
                .session_context
                .validated_plan
                .as_mut()
                .expect("validated")
                .candidate_id = 2;
            *core.pending_files.lock().expect("pending lock") = fixture_core
                .pending_files
                .lock()
                .expect("pending lock")
                .clone();
            core.history
                .lock()
                .expect("history lock")
                .replace_current(state);
            core.execute(request("y"))
        });

        assert_guard_rejected_without_apply(
            &response,
            "CandidateMismatch",
            (&fixture.target, &before),
        );
    }

    #[test]
    fn safe_apply_rejects_missing_preview_snapshot_even_with_validated_plan() {
        let fixture = SafeApplyFixture::new();
        let before = fixture.before_contents();
        let response = fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            let (fixture_core, mut state) = fixture.safe_apply_state(&fixture.planned_contents());
            state.preview_snapshot = None;
            *core.pending_files.lock().expect("pending lock") = fixture_core
                .pending_files
                .lock()
                .expect("pending lock")
                .clone();
            core.history
                .lock()
                .expect("history lock")
                .replace_current(state);
            core.execute(request("y"))
        });

        assert_guard_rejected_without_apply(
            &response,
            "MissingPreviewSnapshot",
            (&fixture.target, &before),
        );
    }

    #[test]
    fn repl_validated_preview_y_passes_apply_guard() {
        let fixture = SafeApplyFixture::new();
        let response = fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            let (fixture_core, state) = fixture.safe_apply_state(&fixture.planned_contents());
            *core.pending_files.lock().expect("pending lock") = fixture_core
                .pending_files
                .lock()
                .expect("pending lock")
                .clone();
            core.history
                .lock()
                .expect("history lock")
                .replace_current(state);
            core.execute(request("y"))
        });

        assert_eq!(response.status, ExecutionStatus::Executed);
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][APPLY_GUARD] rejected=false candidate_id=1 target=File(Cargo.toml) validated_plan_hash=42"))));
        assert!(!response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][APPLY_GUARD] rejected=true"))));
    }

    #[test]
    fn repl_validated_preview_y_executes_safe_apply() {
        let fixture = SafeApplyFixture::new();
        let response = fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            core.execute(request("このプロジェクトの構造を解析して"));
            core.execute(request(
                "このプロジェクトの構造解析結果をもとに、安全な小規模修正プランを作成して。まだ適用しないで",
            ));
            core.execute(request("select 1"));
            core.execute(request("この候補を検証して"));
            core.execute(request("y"))
        });
        let state = response.core_state.as_ref().expect("state");

        assert_eq!(response.status, ExecutionStatus::Executed);
        assert_eq!(state.status, PipelineState::Idle);
        assert!(state.session_context.validated_plan.is_none());
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][SAFE_APPLY_BEGIN]"))));
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][SAFE_APPLY_SUCCESS]"))));
        assert!(
            std::fs::read_to_string(&fixture.target)
                .expect("read")
                .contains("# DBM safe apply candidate 1")
        );
    }

    #[test]
    fn repl_apply_failure_rolls_back() {
        let fixture = SafeApplyFixture::new();
        let before = fixture.before_contents();
        let response = fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            let (fixture_core, state) = fixture.safe_apply_state("DBM_POST_VALIDATION_FAIL\n");
            *core.pending_files.lock().expect("pending lock") = fixture_core
                .pending_files
                .lock()
                .expect("pending lock")
                .clone();
            core.history
                .lock()
                .expect("history lock")
                .replace_current(state);
            core.execute(request("y"))
        });
        let state = response.core_state.as_ref().expect("state");

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert_eq!(state.status, PipelineState::Previewed);
        assert!(state.session_context.validated_plan.is_some());
        assert!(state.session_context.selected_candidate.is_some());
        assert_eq!(
            std::fs::read_to_string(&fixture.target).expect("read"),
            before
        );
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][SAFE_APPLY_ROLLBACK]"))));
    }

    #[test]
    fn preview_cancel_clears_selection() {
        let core = RuntimeCoreBridge::with_defaults();
        set_preview_state_with_context(&core);

        let response = core.execute(request("cancel"));
        let state = response.core_state.expect("state");

        assert_eq!(state.status, PipelineState::Idle);
        assert!(state.session_context.selected_candidate.is_none());
        assert!(state.session_context.validated_plan.is_none());
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][PREVIEW_CONFIRMATION] action=Cancel"))));
    }

    #[test]
    fn no_apply_blocks_governed_transaction_preview() {
        let core = RuntimeCoreBridge::with_defaults();
        set_preview_state_with_context(&core);

        let response = core.execute(request("この変更案について yes/no の確認ではなく、設計上の安全性を文章で評価してください。y や n という文字が含まれていても confirmation として扱わず、自然言語入力として解釈してください。まだapplyしないでください。"));
        let text = event_text(&response.events);

        assert!(
            text.contains("[IR-TRACE][CONFIRMATION_TOKEN_CHECK] result=NotConfirmation"),
            "{text}"
        );
        assert!(text.contains("# Validation Review"), "{text}");
        assert!(text.contains("No files modified"), "{text}");
        assert!(text.contains("No apply executed"), "{text}");
        assert!(!text.contains("preparing governed transaction"), "{text}");
        assert!(!text.contains("transaction active"), "{text}");
        assert!(!text.contains("preview ready"), "{text}");
    }

    #[test]
    fn repl_long_yes_no_sentence_not_target() {
        let core = RuntimeCoreBridge::with_defaults();
        set_preview_state_with_context(&core);

        let response = core.execute(request(long_yes_no_review_input()));
        let text = event_text(&response.events);

        assert!(
            text.contains("[IR-TRACE][TARGET_REJECTED] target=yes/no reason=ConfirmationTokenLike"),
            "{text}"
        );
        assert!(!text.contains("Target: yes/no"), "{text}");
    }

    #[test]
    fn repl_long_yes_no_sentence_not_unresolved() {
        let core = RuntimeCoreBridge::with_defaults();
        set_preview_state_with_context(&core);

        let response = core.execute(request(long_yes_no_review_input()));
        let text = event_text(&response.events);

        assert!(!text.contains("[ERROR] unresolved target"), "{text}");
        assert!(!text.contains("unresolved target"), "{text}");
    }

    #[test]
    fn ambiguous_input_returns_proposal() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("fix parser bug"));

        assert_eq!(response.status, ExecutionStatus::Proposed);
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Proposal { candidates } if !candidates.is_empty())
        ));
        assert!(!response.events.iter().any(|event| matches!(event, CoreEvent::Thinking { summary } if summary == "strategy execution started")));
    }

    #[test]
    fn clear_input_returns_plan_and_result() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("refactor parser.rs"));

        assert_eq!(response.status, ExecutionStatus::Executed);
        assert!(
            response
                .events
                .iter()
                .any(|event| matches!(event, CoreEvent::Plan { .. }))
        );
        assert!(
            response
                .events
                .iter()
                .any(|event| matches!(event, CoreEvent::Execution { .. }))
        );
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Result { message } if message == "core execution completed")));
    }

    #[test]
    fn invalid_input_returns_error() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("git push"));

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Error { message } if message.contains("ExecutionRejected"))));
    }

    #[test]
    fn target_only_input_does_not_enter_coding_pipeline() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("Target: apps/cli/src/git_guard.rs"));

        assert_eq!(response.status, ExecutionStatus::Idle);
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Result { message } if message.contains("[TARGET] context set: apps/cli/src/git_guard.rs"))
        ));
        assert!(
            !response
                .events
                .iter()
                .any(|event| matches!(event, CoreEvent::Plan { .. }))
        );
        assert!(
            !response
                .events
                .iter()
                .any(|event| matches!(event, CoreEvent::Preview { .. }))
        );
        assert!(
            !response
                .events
                .iter()
                .any(|event| matches!(event, CoreEvent::Diff { .. }))
        );
        assert!(!response.events.iter().any(
            |event| matches!(event, CoreEvent::Result { message } if message.contains("Applied"))
        ));
    }

    #[test]
    fn git_status_bypasses_natural_language_pipeline() {
        let temp = tempfile::tempdir().expect("tempdir");
        let response = with_current_dir(temp.path(), || {
            init_git_repo(temp.path());
            let core = RuntimeCoreBridge::with_defaults();
            core.execute(request("git status"))
        });

        assert_eq!(response.status, ExecutionStatus::Executed);
        assert!(!response.events.iter().any(
            |event| matches!(event, CoreEvent::Thinking { summary } if summary == "refining natural language intent")
        ));
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Debug { message } if message.contains("git_execution"))
        ));
    }

    #[test]
    fn git_add_dot_is_rejected() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("git add ."));

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Error { message } if message.contains("git add . is rejected"))
        ));
    }

    #[test]
    fn dirty_tree_guard_rejects_unrelated_dirty_files_for_scoped_add() {
        let temp = tempfile::tempdir().expect("tempdir");
        init_git_repo(temp.path());
        std::fs::write(temp.path().join("tracked.txt"), "target change\n").expect("target");
        std::fs::write(temp.path().join("other.txt"), "other\n").expect("other");
        std::process::Command::new("git")
            .args(["add", "other.txt"])
            .current_dir(temp.path())
            .output()
            .expect("git add other");
        std::process::Command::new("git")
            .args(["commit", "-m", "other"])
            .current_dir(temp.path())
            .output()
            .expect("git commit other");
        std::fs::write(temp.path().join("other.txt"), "dirty other\n").expect("dirty other");

        let response = with_current_dir(temp.path(), || {
            let core = RuntimeCoreBridge::with_defaults();
            core.execute(request("git add tracked.txt"))
        });

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Error { message } if message.contains("working tree dirty"))
        ));
    }

    #[test]
    fn same_git_input_produces_same_operation_sequence_and_targets() {
        let left = crate::git::commands::parse_git_command("git add tracked.txt").expect("left");
        let right = crate::git::commands::parse_git_command("git add tracked.txt").expect("right");

        assert_eq!(left.canonical(), right.canonical());
        assert_eq!(left.targets(), right.targets());
        assert_eq!(git_command_policy(&left), git_command_policy(&right));
    }

    #[test]
    fn dirty_tree_policy_ignores_dbm_runtime_paths() {
        let policy = DirtyTreePolicy::default();

        assert!(policy.is_ignored(Path::new(".dbm/replay/cache.json")));
        assert!(policy.is_ignored(Path::new("target/tmp/dbm-cache")));
        assert!(!policy.is_ignored(Path::new("apps/cli/src/core.rs")));
    }

    #[test]
    fn same_transaction_input_produces_same_transaction_record() {
        let mut left = ExecutionTransaction::new("tx-fixed".to_string(), 100);
        left.mark_applied();
        left.mark_staged(vec![PathBuf::from("tracked.txt")]);
        left.mark_committed("abc1234".to_string());
        left.finalize(200);

        let mut right = ExecutionTransaction::new("tx-fixed".to_string(), 100);
        right.mark_applied();
        right.mark_staged(vec![PathBuf::from("tracked.txt")]);
        right.mark_committed("abc1234".to_string());
        right.finalize(200);

        assert_eq!(
            crate::git::transaction::TransactionRecord::from(&left),
            crate::git::transaction::TransactionRecord::from(&right)
        );
        assert_eq!(
            transaction_record_json(&left),
            transaction_record_json(&right)
        );
    }

    #[test]
    fn recovery_fail_safe_finalizes_unfinished_transaction() {
        let mut transaction = ExecutionTransaction::new("tx-recovery".to_string(), 100);
        transaction.mark_applied();
        transaction.mark_staged(vec![PathBuf::from("tracked.txt")]);
        transaction.fail_safe_finalize_after_recovery(300);

        assert_eq!(transaction.finalized_at, Some(300));
    }

    #[test]
    fn candidate_to_plan_requires_non_empty_steps() {
        let empty = ExecutionPlanCandidate {
            id: 1,
            summary: "empty".to_string(),
            steps: vec![],
            target: None,
            expected_effects: vec![],
            risks: vec![],
            confidence: 0.0,
            score: 0.0,
        };
        assert!(candidate_to_execution_plan(&empty).is_err());

        let valid = ExecutionPlanCandidate::from_ops(
            1,
            "build",
            vec![ExecutionOp::RuntimePhase("cargo build".to_string())],
            None,
        );
        assert!(candidate_to_execution_plan(&valid).is_ok());
    }

    #[test]
    fn clarification_target_falls_back_to_cli_src_path() {
        let root =
            std::env::temp_dir().join(format!("dbm-clarification-target-{}", uuid::Uuid::new_v4()));
        let cli_src = root.join("apps/cli/src");
        std::fs::create_dir_all(&cli_src).expect("create cli src");
        std::fs::write(cli_src.join("coding.rs"), "fn marker() {}\n").expect("write target");

        let (relative, path) = resolve_clarification_target(&root, "src/coding.rs")
            .expect("fallback target should resolve");

        assert_eq!(relative, "apps/cli/src/coding.rs");
        assert!(path.ends_with("apps/cli/src/coding.rs"));
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn append_comment_line_adds_one_comment_line() {
        let updated = append_comment_line("fn marker() {}\n", "src/coding.rs");
        assert!(updated.ends_with("// DBM clarification execution guarantee\n"));
        assert_eq!(updated.lines().count(), 2);
    }

    #[test]
    fn append_comment_line_is_idempotent() {
        let once = append_comment_line("fn marker() {}\n", "src/coding.rs");
        let twice = append_comment_line(&once, "src/coding.rs");

        assert_eq!(twice, once);
        assert_eq!(
            twice
                .lines()
                .filter(|line| line.trim() == "// DBM clarification execution guarantee")
                .count(),
            1
        );
    }

    #[test]
    fn clarification_bypass_applies_through_unified_apply_gate_idempotently() {
        let root =
            std::env::temp_dir().join(format!("dbm-clarification-apply-{}", uuid::Uuid::new_v4()));
        let cli_src = root.join("apps/cli/src");
        std::fs::create_dir_all(&cli_src).expect("create cli src");
        std::fs::write(cli_src.join("coding.rs"), "fn marker() {}\n").expect("write target");
        let core = RuntimeCoreBridge::with_defaults();
        let input = "add comment to src/coding.rs";

        for _ in 0..3 {
            let request = InternalRequest {
                id: next_request_id(),
                input: input.to_string(),
                kind: CoreRequestKind::NaturalLanguage,
                context: ExecutionContext {
                    working_dir: root.clone(),
                    pipeline_state: PipelineState::Idle,
                    design_snapshot: None,
                    current_proposals: None,
                },
            };
            let response = core.execute_clarification_bypassed_pipeline(
                Intent::new(input),
                request,
                Vec::new(),
                "src/coding.rs".to_string(),
            );
            assert_eq!(response.status, ExecutionStatus::Executed);
        }

        let content = std::fs::read_to_string(cli_src.join("coding.rs")).expect("read target");
        assert_eq!(
            content
                .lines()
                .filter(|line| line.trim() == "// DBM clarification execution guarantee")
                .count(),
            1
        );
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn unified_apply_rejects_pending_file_outside_intent_target() {
        let root = std::env::temp_dir().join(format!("dbm-apply-target-{}", uuid::Uuid::new_v4()));
        let cli_src = root.join("apps/cli/src");
        std::fs::create_dir_all(&cli_src).expect("create cli src");
        std::fs::write(cli_src.join("coding.rs"), "fn coding() {}\n").expect("write coding");
        std::fs::write(cli_src.join("app.rs"), "fn app() {}\n").expect("write app");
        let core = RuntimeCoreBridge::with_defaults();
        *core.pending_files.lock().expect("pending lock") = vec![PendingFile {
            path: "apps/cli/src/app.rs".to_string(),
            content: "fn app() { let _ = 1; }\n".to_string(),
            content_checksum: checksum_bytes("fn app() { let _ = 1; }\n".as_bytes()),
        }];

        let request = InternalRequest {
            id: next_request_id(),
            input: "add comment to apps/cli/src/coding.rs".to_string(),
            kind: CoreRequestKind::Apply,
            context: ExecutionContext {
                working_dir: root.clone(),
                pipeline_state: PipelineState::Previewed,
                design_snapshot: None,
                current_proposals: None,
            },
        };
        let response = core.apply(&request);

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(
            response
                .events
                .iter()
                .any(|event| matches!(event, CoreEvent::Error { message } if message.contains("TargetViolation")))
        );
        assert_eq!(
            std::fs::read_to_string(cli_src.join("app.rs")).expect("read app"),
            "fn app() {}\n"
        );
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn select_candidate_returns_preview_and_confirm_choices() {
        let core = RuntimeCoreBridge::with_defaults();
        let candidate = ExecutionPlanCandidate::from_ops(
            1,
            "Fix parser.rs",
            vec![ExecutionOp::RuntimePhase("fix parser.rs".to_string())],
            None,
        );
        let response = core.execute(request_with_state(
            &core,
            "select 1",
            PipelineState::Proposed,
            Some(vec![candidate]),
        ));

        assert_eq!(response.status, ExecutionStatus::Planned);
        assert!(
            response
                .events
                .iter()
                .any(|event| matches!(event, CoreEvent::Preview { .. }))
        );
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Next { actions } if actions == &vec!["y".to_string(), "n".to_string()])));
    }

    #[test]
    fn proposals_command_redisplays_active_candidates() {
        let core = RuntimeCoreBridge::with_defaults();
        let candidate = ExecutionPlanCandidate::from_ops(
            1,
            "Fix parser.rs",
            vec![ExecutionOp::RuntimePhase("fix parser.rs".to_string())],
            None,
        );
        let response = core.execute(request_with_state(
            &core,
            "/proposals",
            PipelineState::Previewed,
            Some(vec![candidate]),
        ));

        assert_eq!(response.status, ExecutionStatus::Proposed);
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Proposal { candidates } if candidates.len() == 1)
        ));
    }

    #[test]
    fn slash_structure_executes_through_design_adapter() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("/structure view ."));

        assert_eq!(response.status, ExecutionStatus::Executed);
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Result { message } if message == "Structure view for .")
        ));
    }

    #[test]
    fn error_response_includes_recovery_candidates() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("git push"));

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::ErrorRecovery { candidates } if !candidates.is_empty())
        ));
    }

    #[test]
    fn compare_proposals_returns_comparison_debug() {
        let core = RuntimeCoreBridge::with_defaults();
        let candidate = ExecutionPlanCandidate::from_ops(
            1,
            "Fix parser.rs",
            vec![ExecutionOp::RuntimePhase("fix parser.rs".to_string())],
            None,
        );
        let response = core.execute(request_with_state(
            &core,
            "compare",
            PipelineState::Proposed,
            Some(vec![candidate]),
        ));

        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Debug { message } if message.contains("confidence="))
        ));
    }

    // ── History unit tests (Phase 4.5) ───────────────────────────────────────

    #[test]
    fn history_push_increments_version_and_moves_cursor() {
        let mut h = History::default();
        assert_eq!(h.cursor(), 0);
        assert_eq!(h.current().version, 0);

        let s1 = CoreState::default();
        h.push(s1);
        assert_eq!(h.cursor(), 1);
        assert_eq!(h.current().version, 1);

        let s2 = CoreState::default();
        h.push(s2);
        assert_eq!(h.cursor(), 2);
        assert_eq!(h.current().version, 2);
    }

    #[test]
    fn history_undo_moves_cursor_back_without_mutation() {
        let mut h = History::default();
        h.push(CoreState {
            status: PipelineState::Proposed,
            ..CoreState::default()
        });
        h.push(CoreState {
            status: PipelineState::Planned,
            ..CoreState::default()
        });
        assert_eq!(h.cursor(), 2);

        let restored = h.undo().expect("undo should succeed");
        assert_eq!(restored.status, PipelineState::Proposed);
        assert_eq!(h.cursor(), 1);
        // entries are still all there
        assert_eq!(h.entries().len(), 3);
    }

    #[test]
    fn history_undo_at_root_returns_none() {
        let mut h = History::default();
        assert!(h.undo().is_none());
    }

    #[test]
    fn history_jump_moves_cursor_to_version() {
        let mut h = History::default();
        h.push(CoreState {
            status: PipelineState::Proposed,
            ..CoreState::default()
        });
        h.push(CoreState {
            status: PipelineState::Planned,
            ..CoreState::default()
        });
        h.push(CoreState {
            status: PipelineState::Previewed,
            ..CoreState::default()
        });

        let jumped = h.jump(1).expect("jump to v1");
        assert_eq!(jumped.status, PipelineState::Proposed);
        assert_eq!(h.cursor(), 1);
    }

    #[test]
    fn history_jump_unknown_version_returns_none() {
        let mut h = History::default();
        assert!(h.jump(99).is_none());
    }

    #[test]
    fn history_replay_from_truncates_forward_chain() {
        let mut h = History::default();
        h.push(CoreState {
            status: PipelineState::Proposed,
            ..CoreState::default()
        }); // v1
        h.push(CoreState {
            status: PipelineState::Planned,
            ..CoreState::default()
        }); // v2
        h.push(CoreState {
            status: PipelineState::Previewed,
            ..CoreState::default()
        }); // v3

        // replay from v1: truncate v2, v3 and return v1 as branch root
        let root = h.replay_from(1).expect("replay from v1");
        assert_eq!(root.status, PipelineState::Proposed);
        assert_eq!(h.cursor(), 1);
        assert_eq!(h.entries().len(), 2, "v2 and v3 should be truncated");

        // push after replay creates a new branch
        h.push(CoreState {
            status: PipelineState::Idle,
            ..CoreState::default()
        });
        assert_eq!(h.cursor(), 2);
        assert_eq!(h.current().version, 4);
    }

    #[test]
    fn history_len_is_limited_to_max_history() {
        let mut h = History::with_limits(Limits {
            max_history: 3,
            ..Limits::default()
        });
        for i in 0..5 {
            h.push(CoreState {
                status: PipelineState::Proposed,
                depth: i,
                ..CoreState::default()
            });
        }

        assert_eq!(h.entries().len(), 3);
        assert_eq!(h.cursor(), 2);
        assert_eq!(h.current().version, 5);
    }

    #[test]
    fn history_push_after_undo_truncates_forward() {
        let mut h = History::default();
        h.push(CoreState::default()); // v1
        h.push(CoreState::default()); // v2
        h.undo(); // cursor → v1

        // new push should truncate v2 and create v2 as new entry
        h.push(CoreState {
            status: PipelineState::Planned,
            ..CoreState::default()
        });
        assert_eq!(h.entries().len(), 3); // root + v1 (the undo base) + new
        assert_eq!(h.current().status, PipelineState::Planned);
    }

    #[test]
    fn execute_attaches_core_state_to_response() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("fix parser bug"));
        assert!(
            response.core_state.is_some(),
            "every execute() must return a core_state"
        );
    }

    #[test]
    fn undo_command_returns_core_state_without_push() {
        let core = RuntimeCoreBridge::with_defaults();
        // First execute something to have a non-root history entry
        core.execute(request("fix parser bug"));
        let before_len = core.history.lock().unwrap().entries().len();

        let response = core.execute(request("undo"));
        let after_len = core.history.lock().unwrap().entries().len();

        // undo is a navigation command — it must NOT push a new entry
        assert_eq!(before_len, after_len, "undo must not push to history");
        assert!(response.core_state.is_some());
    }

    #[test]
    fn max_depth_returns_result_without_expanding() {
        let core = core_with_limits(Limits {
            max_depth: 1,
            ..Limits::default()
        });

        let first = core.execute(request("fix parser bug"));
        assert_eq!(first.core_state.as_ref().map(|s| s.depth), Some(1));

        let response = core.execute(request("refactor parser.rs"));
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Result { message } if message == "Max depth reached")
        ));
        assert_eq!(response.core_state.as_ref().map(|s| s.depth), Some(1));
    }

    #[test]
    fn reselect_is_disabled_at_max_depth() {
        let core = core_with_limits(Limits {
            max_depth: 1,
            ..Limits::default()
        });
        core.execute(request("fix parser bug"));
        let candidate = ExecutionPlanCandidate::from_ops(
            1,
            "Fix parser.rs",
            vec![ExecutionOp::RuntimePhase("fix parser.rs".to_string())],
            None,
        );

        let response = core.execute(request_with_state(
            &core,
            "reselect",
            PipelineState::Proposed,
            Some(vec![candidate]),
        ));

        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Error { message } if message == "Reselect disabled at max depth")
        ));
    }

    fn seed_replay_history(core: &RuntimeCoreBridge) {
        let mut h = core.history.lock().unwrap();
        h.push(CoreState {
            status: PipelineState::Proposed,
            ..CoreState::default()
        });
        h.push(CoreState {
            status: PipelineState::Planned,
            ..CoreState::default()
        });
        h.push(CoreState {
            status: PipelineState::Previewed,
            ..CoreState::default()
        });
    }

    #[test]
    fn replay_invalid_target_rejected() {
        let core = RuntimeCoreBridge::with_defaults();
        seed_replay_history(&core);
        let before = core.history.lock().unwrap().entries().to_vec();

        let response = core.execute(request("replay 99"));

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Error { message } if message == "Replay target does not exist")
        ));
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][REPLAY_TARGET_REJECTED]"))
        ));
        assert_eq!(core.history.lock().unwrap().entries(), before.as_slice());
    }

    #[test]
    fn replay_zero_rejected() {
        let core = RuntimeCoreBridge::with_defaults();
        seed_replay_history(&core);

        let response = core.execute(request("replay 0"));

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Error { message } if message == "Replay target does not exist")
        ));
    }

    #[test]
    fn replay_future_version_rejected() {
        let core = RuntimeCoreBridge::with_defaults();
        seed_replay_history(&core);

        let response = core.execute(request("replay 4"));

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Error { message } if message == "Replay target does not exist")
        ));
    }

    #[test]
    fn replay_distance_is_limited() {
        let core = core_with_limits(Limits {
            max_replay_steps: 1,
            max_depth: 10,
            ..Limits::default()
        });
        seed_replay_history(&core);

        let response = core.execute(request("replay 1"));

        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Error { message } if message == "Replay limit exceeded")
        ));
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][REPLAY_DISTANCE_VALIDATE]"))
        ));
    }

    #[test]
    fn replay_limit_exceeded() {
        let core = core_with_limits(Limits {
            max_replay_steps: 1,
            max_depth: 10,
            ..Limits::default()
        });
        seed_replay_history(&core);

        let response = core.execute(request("replay 1"));

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Error { message } if message == "Replay limit exceeded")
        ));
    }

    #[test]
    fn replay_parse_failure() {
        let core = RuntimeCoreBridge::with_defaults();
        seed_replay_history(&core);

        let response = core.execute(request("replay nope"));

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Error { message } if message == "Invalid replay target")
        ));
    }

    #[test]
    fn replay_validation_failure_preserves_pipeline() {
        let core = RuntimeCoreBridge::with_defaults();
        let mut state = state_with_plan_context();
        state.status = PipelineState::Previewed;
        core.history.lock().unwrap().push(state.clone());

        let response = core.execute(request("replay 99"));

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert_eq!(
            response
                .core_state
                .as_ref()
                .map(|state| state.status.clone()),
            Some(PipelineState::Previewed)
        );
        assert_eq!(
            core.history.lock().unwrap().current().status,
            PipelineState::Previewed
        );
    }

    #[test]
    fn replay_validation_failure_preserves_context() {
        let core = RuntimeCoreBridge::with_defaults();
        let mut state = state_with_plan_context();
        state.status = PipelineState::Previewed;
        let expected_context = state.session_context.clone();
        core.history.lock().unwrap().push(state);

        let response = core.execute(request("replay nope"));

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert_eq!(
            response
                .core_state
                .as_ref()
                .map(|state| &state.session_context),
            Some(&expected_context)
        );
        assert_eq!(
            core.history.lock().unwrap().current().session_context,
            expected_context
        );
    }

    #[test]
    fn replay_validation_failure_not_idle() {
        let core = RuntimeCoreBridge::with_defaults();
        seed_replay_history(&core);

        let response = core.execute(request("replay 99"));

        assert_ne!(response.status, ExecutionStatus::Idle);
        assert_eq!(
            response
                .core_state
                .as_ref()
                .map(|state| state.status.clone()),
            Some(PipelineState::Previewed)
        );
    }

    #[test]
    fn command_dispatch_replay_not_confirmation() {
        let core = RuntimeCoreBridge::with_defaults();
        seed_replay_history(&core);

        let response = core.execute(request_with_state(
            &core,
            "replay nope",
            PipelineState::Previewed,
            None,
        ));

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Error { message } if message == "Invalid replay target")
        ));
        assert!(!response.events.iter().any(
            |event| matches!(event, CoreEvent::Debug { message } if message.contains("[IR-TRACE][PREVIEW_CONFIRMATION]"))
        ));
    }

    #[test]
    fn command_dispatch_replay_not_nl() {
        let core = RuntimeCoreBridge::with_defaults();
        seed_replay_history(&core);

        let response = core.execute(request("replay nope"));
        let text = event_text(&response.events);

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(text.contains("Invalid replay target"), "{text}");
        assert!(!text.contains("LANGUAGE_CORE"), "{text}");
        assert!(!text.contains("ClarificationRequired"), "{text}");
    }

    #[test]
    fn command_dispatch_preview_not_clarification() {
        let core = RuntimeCoreBridge::with_defaults();

        let response = core.execute(request("preview"));
        let text = event_text(&response.events);

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(text.contains("preview requires Planned state"), "{text}");
        assert!(!text.contains("ClarificationRequired"), "{text}");
        assert!(!text.contains("LANGUAGE_CORE"), "{text}");
    }

    #[test]
    fn command_dispatch_failure_not_idle() {
        let core = RuntimeCoreBridge::with_defaults();

        let response = core.execute(request("apply"));

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert_ne!(response.status, ExecutionStatus::Idle);
    }

    // ── LanguageCoreToIrAdapter 統合テスト (spec §11.2) ──────────────────────

    /// spec §11.2 テスト 1: 日本語のプロジェクト構造解析が InvalidInput にならない
    ///
    /// 「このプロジェクトの構造を解析して」は DefaultIntentRefiner の ASCII-only
    /// normalizer を通過せずに Adapter で処理されるため、InvalidInput は返さない。
    #[test]
    fn nl_project_structure_request_does_not_return_invalid_input() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("このプロジェクトの構造を解析して"));

        assert_ne!(
            response.status,
            ExecutionStatus::Failed,
            "Japanese analyze input must not produce Failed status"
        );
        let has_invalid_input_error = response.events.iter().any(
            |event| matches!(event, CoreEvent::Error { message } if message.contains("InvalidInput")),
        );
        assert!(
            !has_invalid_input_error,
            "Response must not contain InvalidInput error for Japanese analyze input"
        );
    }

    #[test]
    fn repl_session_context_stores_previous_analysis() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("このプロジェクトの構造を解析して"));
        let state = response.core_state.as_ref().expect("core state");

        assert!(state.session_context.previous_analysis_context.is_some());
        assert_eq!(
            state
                .session_context
                .previous_analysis_context
                .as_ref()
                .expect("analysis")
                .target,
            IrTarget::WorkspaceRoot
        );
    }

    #[test]
    fn repl_session_context_loads_previous_analysis() {
        let core = RuntimeCoreBridge::with_defaults();
        core.execute(request("このプロジェクトの構造を解析して"));

        let response = core.execute(request(
            "このプロジェクトの構造解析結果をもとに、安全な小規模修正プランを作成して。まだ適用しないで",
        ));

        assert!(
            response
                .events
                .iter()
                .any(|event| matches!(event, CoreEvent::Debug { message }
                if message.contains("[IR-TRACE][CONTEXT_LOAD]")
                    && message.contains("previous_analysis_context=Some")))
        );
        assert!(
            response
                .events
                .iter()
                .any(|event| matches!(event, CoreEvent::Debug { message }
                if message.contains("[IR-TRACE][CONTEXT_RESOLUTION]")
                    && message.contains("previous_context_used=true")))
        );
    }

    #[test]
    fn workspace_root_plan_generates_narrow_candidates() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            core.execute(request("このプロジェクトの構造を解析して"));

            let response = core.execute(request(
                "このプロジェクトの構造解析結果をもとに、安全な小規模修正プランを作成して。まだ適用しないで",
            ));
            let state = response.core_state.as_ref().expect("core state");
            let plan = state
                .session_context
                .previous_plan_context
                .as_ref()
                .expect("plan context");

            assert_eq!(plan.target, IrTarget::WorkspaceRoot);
            assert!(plan.candidates.len() >= 2, "{:?}", plan.candidates);
            assert!(plan.candidates.iter().all(|candidate| matches!(
                candidate.target,
                NarrowTarget::File(_) | NarrowTarget::Module(_) | NarrowTarget::Symbol(_)
            )));
        })
    }

    fn long_analyze_request(
        primary_request: &str,
        referenced_concepts: &str,
        safety_constraints: &str,
    ) -> String {
        format!("{primary_request}。{referenced_concepts}。{safety_constraints}。")
    }

    fn long_analyze_request_with_plan_terms() -> String {
        long_analyze_request(
            "このプロジェクト全体の構造を解析してください",
            "現時点でInputから構造理解、修正プラン生成、候補選択、検証、Apply Guardまでの接続に問題がないかを整理してください",
            "まだ修正、apply、git操作、外部コマンド実行は行わないでください",
        )
    }

    fn long_plan_request_with_context_reference() -> String {
        [
            "先ほどのプロジェクト構造解析結果をもとに、確認された問題点を整理し、安全な小規模修正プランを作成してください",
            "ただし、まだファイル変更、apply、git操作、外部コマンド実行は行わず、候補ごとに対象ファイル、想定変更、リスク、検証方法だけを提示してください",
        ]
        .join("。")
    }

    fn analysis_to_candidate_proposal_input() -> &'static str {
        "この解析結果を元に、最小で安全な修正候補を3つ提案してください。まだ修正、apply、git操作、外部コマンド実行は行わないでください。"
    }

    fn has_trace_value(response: &CoreResponse, stage: &str, key: &str, value: &str) -> bool {
        response.events.iter().any(|event| {
            matches!(
                event,
                CoreEvent::Debug { message }
                    if message.contains(&format!("\"stage\":\"{stage}\""))
                        && message.contains(&format!("\"{key}\":\"{value}\""))
            )
        })
    }

    #[test]
    fn repl_long_analyze_request_routes_to_analyze_project() {
        let core = RuntimeCoreBridge::with_defaults();
        let input = long_analyze_request_with_plan_terms();
        let response = core.execute(request(&input));

        assert!(has_trace_value(
            &response,
            "LANGUAGE_CORE",
            "intent",
            "AnalyzeProject"
        ));
    }

    #[test]
    fn repl_long_analyze_request_is_read_only() {
        let core = RuntimeCoreBridge::with_defaults();
        let input = long_analyze_request_with_plan_terms();
        let response = core.execute(request(&input));

        assert!(has_trace_value(&response, "ADAPTER", "mode", "ReadOnly"));
    }

    #[test]
    fn repl_long_analyze_request_targets_workspace_root() {
        let core = RuntimeCoreBridge::with_defaults();
        let input = long_analyze_request_with_plan_terms();
        let response = core.execute(request(&input));

        assert!(has_trace_value(
            &response,
            "ADAPTER",
            "target",
            "WorkspaceRoot"
        ));
    }

    #[test]
    fn repl_long_analyze_with_plan_terms_does_not_generate_plan() {
        let core = RuntimeCoreBridge::with_defaults();
        let input = long_analyze_request_with_plan_terms();
        let response = core.execute(request(&input));

        assert!(!has_trace_value(
            &response,
            "LANGUAGE_CORE",
            "intent",
            "GenerateChangePlan"
        ));
    }

    #[test]
    fn repl_long_plan_request_uses_previous_analysis_context() {
        let core = RuntimeCoreBridge::with_defaults();
        let analyze_input = long_analyze_request_with_plan_terms();
        let plan_input = long_plan_request_with_context_reference();
        core.execute(request(&analyze_input));
        let response = core.execute(request(&plan_input));

        assert!(response.events.iter().any(|event| matches!(
            event,
            CoreEvent::Debug { message }
                if message.contains("[IR-TRACE][CONTEXT_RESOLUTION]")
                    && message.contains("previous_context_used=true")
                    && message.contains("target=WorkspaceRoot")
        )));
        assert!(has_trace_value(
            &response,
            "ADAPTER",
            "action",
            "GenerateChangePlan"
        ));
        assert!(has_trace_value(
            &response,
            "ADAPTER",
            "target",
            "WorkspaceRoot"
        ));
    }

    #[test]
    fn analysis_context_candidate_proposal_intent() {
        let core = RuntimeCoreBridge::with_defaults();
        core.execute(request("このプロジェクト全体の構造を解析してください。まだ修正、apply、git操作、外部コマンド実行は行わないでください。"));

        let response = core.execute(request(analysis_to_candidate_proposal_input()));
        let state = response.core_state.as_ref().expect("core state");

        assert!(has_trace_value(
            &response,
            "LANGUAGE_CORE",
            "intent",
            "GenerateChangePlan"
        ));
        assert!(has_trace_value(
            &response,
            "ADAPTER",
            "action",
            "GenerateChangePlan"
        ));
        assert!(has_trace_value(&response, "ADAPTER", "mode", "PlanOnly"));
        assert!(has_trace_value(
            &response,
            "ADAPTER",
            "target",
            "WorkspaceRoot"
        ));
        assert!(state.session_context.previous_plan_context.is_some());
        assert_eq!(state.status, PipelineState::Proposed);
    }

    #[test]
    fn analysis_to_candidate_proposal_does_not_reanalyze_project() {
        let core = RuntimeCoreBridge::with_defaults();
        core.execute(request("このプロジェクト全体の構造を解析してください。まだ修正、apply、git操作、外部コマンド実行は行わないでください。"));

        let response = core.execute(request(analysis_to_candidate_proposal_input()));

        assert!(!has_trace_value(
            &response,
            "LANGUAGE_CORE",
            "intent",
            "AnalyzeProject"
        ));
        assert!(response.events.iter().any(|event| matches!(
            event,
            CoreEvent::Debug { message }
                if message.contains("[IR-TRACE][CONTEXT_STORE] kind=plan")
        )));
    }

    #[test]
    fn analysis_to_candidate_proposal_preserves_no_apply_constraints() {
        let core = RuntimeCoreBridge::with_defaults();
        core.execute(request("このプロジェクト全体の構造を解析してください。まだ修正、apply、git操作、外部コマンド実行は行わないでください。"));

        let response = core.execute(request(analysis_to_candidate_proposal_input()));
        let text = event_text(&response.events);

        assert!(
            text.contains("[IR-TRACE][SAFETY_CONSTRAINTS] no_apply=true no_file_write=true no_git_operation=true no_external_command=true"),
            "{text}"
        );
    }

    #[test]
    fn repl_analysis_to_candidate_proposal_allows_select() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            core.execute(request("このプロジェクト全体の構造を解析してください。まだ修正、apply、git操作、外部コマンド実行は行わないでください。"));
            core.execute(request(analysis_to_candidate_proposal_input()));

            let response = core.execute(request("select 1"));
            let text = event_text(&response.events);

            assert!(!text.contains("Cannot select in current state"), "{text}");
            assert!(
                text.contains("[IR-TRACE][CONTEXT_STORE] kind=selection candidate_id=1"),
                "{text}"
            );
            assert!(response.events.iter().any(|event| matches!(
                event,
                CoreEvent::Pipeline { state } if state == PipelineState::Previewed.label()
            )));
        })
    }

    #[test]
    fn repl_long_apply_prohibited_does_not_apply() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request(
            "この候補が安全であれば適用してもよいか検討してください。ただし、この入力ではまだapplyしないでください。まず検証結果だけを表示し、validated_planが作成されてもファイル変更は次の明示的な確認まで停止してください。",
        ));

        assert!(!has_trace_value(&response, "ADAPTER", "action", "Apply"));
    }

    fn no_apply_validated_plan_input() -> &'static str {
        "この候補が安全であれば適用してもよいか検討してください。ただし、この入力ではまだapplyしないでください。まず検証結果だけを表示し、validated_planが作成されてもファイル変更は次の明示的な確認まで停止してください。"
    }

    fn long_yes_no_review_input() -> &'static str {
        "この変更案について yes/no の確認ではなく、設計上の安全性を文章で評価してください。y や n という文字が含まれていても confirmation として扱わず、自然言語入力として解釈してください。まだapplyしないでください。"
    }

    fn prepare_selected_candidate(core: &RuntimeCoreBridge) {
        core.execute(request("このプロジェクトの構造を解析して"));
        core.execute(request(
            "このプロジェクトの構造解析結果をもとに、安全な小規模修正プランを作成して。まだ適用しないで",
        ));
        core.execute(request("select 1"));
    }

    fn prepare_validated_plan(core: &RuntimeCoreBridge) {
        prepare_selected_candidate(core);
        core.execute(request("選択済みの候補について検証してください。対象ファイルがworkspace内に存在すること、WorkspaceRootへの直接Applyではないこと、変更内容が破壊的でないこと、既存のPlan hashと候補IDが一致していること、validated_planなしにApplyへ進まないことを確認してください。検証結果だけを表示し、まだファイル変更は行わないでください。"));
    }

    #[test]
    fn no_apply_apply_with_validated_plan_downgrades_to_review() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            prepare_validated_plan(&core);

            let response = core.execute(request(no_apply_validated_plan_input()));

            assert!(response.events.iter().any(|event| matches!(
                event,
                CoreEvent::Debug { message }
                    if message.contains("[IR-TRACE][INTENT_DOWNGRADE] from=Apply to=ReviewValidatedPlan reason=NoApplyWithValidatedPlan")
            )));
            assert!(has_trace_value(
                &response,
                "ADAPTER",
                "action",
                "ReviewValidatedPlan"
            ));
        })
    }

    #[test]
    fn confirmation_like_target_with_validated_plan_falls_back_to_review_validated_plan() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            prepare_validated_plan(&core);

            let response = core.execute(request(long_yes_no_review_input()));
            let text = event_text(&response.events);

            assert!(
                text.contains("[IR-TRACE][TARGET_REJECTED] target=yes/no reason=ConfirmationTokenLike"),
                "{text}"
            );
            assert!(
                text.contains(
                    "[IR-TRACE][FALLBACK_PRIORITY] reason=ConfirmationTokenLike before=UnresolvedTarget"
                ),
                "{text}"
            );
            assert!(text.contains("[IR-TRACE][INTENT_DOWNGRADE] to=ReviewValidatedPlan reason=RejectedConfirmationLikeTargetWithValidatedPlan"), "{text}");
            assert!(
                text.contains(
                    "[IR-TRACE][VALIDATED_PLAN_PRESERVE] candidate_id=1 target=File(Cargo.toml)"
                ),
                "{text}"
            );
            assert!(text.contains("# Validation Review"), "{text}");
            assert!(text.contains("Apply status: Deferred"), "{text}");
            assert!(!text.contains("[ERROR] unresolved target"), "{text}");
        })
    }

    #[test]
    fn confirmation_like_target_with_selected_candidate_falls_back_to_validate_plan() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            prepare_selected_candidate(&core);

            let response = core.execute(request(long_yes_no_review_input()));
            let text = event_text(&response.events);

            assert!(
                text.contains(
                    "[IR-TRACE][FALLBACK_PRIORITY] reason=ConfirmationTokenLike before=UnresolvedTarget"
                ),
                "{text}"
            );
            assert!(text.contains("[IR-TRACE][INTENT_DOWNGRADE] to=ValidatePlan reason=RejectedConfirmationLikeTargetWithSelectedCandidate"), "{text}");
            assert!(has_trace_value(
                &response,
                "ADAPTER",
                "action",
                "ValidatePlan"
            ));
            assert!(text.contains("# Plan Validation"), "{text}");
            assert!(!text.contains("[ERROR] unresolved target"), "{text}");
        })
    }

    #[test]
    fn confirmation_like_target_without_context_falls_back_to_review_safety() {
        let core = RuntimeCoreBridge::with_defaults();

        let response = core.execute(request(long_yes_no_review_input()));
        let text = event_text(&response.events);

        assert!(
            text.contains(
                "[IR-TRACE][FALLBACK_PRIORITY] reason=ConfirmationTokenLike before=UnresolvedTarget"
            ),
            "{text}"
        );
        assert!(text.contains("[IR-TRACE][INTENT_DOWNGRADE] to=ReviewSafety reason=RejectedConfirmationLikeTargetWithoutContext"), "{text}");
        assert!(text.contains("# Safety Review"), "{text}");
        assert!(
            text.contains("Target extraction: Rejected confirmation-like token"),
            "{text}"
        );
        assert!(text.contains("No files modified"), "{text}");
        assert!(text.contains("No apply executed"), "{text}");
        assert!(!text.contains("[ERROR] unresolved target"), "{text}");
    }

    #[test]
    fn confirmation_like_failure_never_returns_unresolved_target() {
        let core = RuntimeCoreBridge::with_defaults();
        prepare_validated_plan(&core);

        let response = core.execute(request(long_yes_no_review_input()));
        let text = event_text(&response.events);

        assert!(!text.contains("[ERROR] unresolved target"), "{text}");
        assert!(!text.contains("unresolved target"), "{text}");
    }

    #[test]
    fn confirmation_like_target_rejection_preserves_validated_plan() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            prepare_validated_plan(&core);
            let before = core.history.lock().unwrap().current().clone();

            let response = core.execute(request(long_yes_no_review_input()));
            let after = response.core_state.expect("state");

            assert_eq!(
                before.session_context.validated_plan,
                after.session_context.validated_plan
            );
        })
    }

    #[test]
    fn confirmation_like_target_rejection_preserves_selected_candidate() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            prepare_validated_plan(&core);

            let response = core.execute(request(long_yes_no_review_input()));
            let state = response.core_state.expect("state");

            assert!(state.session_context.selected_candidate.is_some());
        })
    }

    #[test]
    fn no_apply_apply_with_selected_candidate_downgrades_to_validate() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            prepare_selected_candidate(&core);

            let response = core.execute(request(no_apply_validated_plan_input()));

            assert!(response.events.iter().any(|event| matches!(
                event,
                CoreEvent::Debug { message }
                    if message.contains("[IR-TRACE][INTENT_DOWNGRADE] from=Apply to=ValidatePlan reason=NoApplyWithSelectedCandidate")
            )));
            assert!(has_trace_value(
                &response,
                "ADAPTER",
                "action",
                "ValidatePlan"
            ));
        })
    }

    #[test]
    fn no_apply_apply_without_selection_downgrades_to_plan() {
        let core = RuntimeCoreBridge::with_defaults();

        let response = core.execute(request(no_apply_validated_plan_input()));

        assert!(response.events.iter().any(|event| matches!(
            event,
            CoreEvent::Debug { message }
                if message.contains("[IR-TRACE][INTENT_DOWNGRADE] from=Apply to=GenerateChangePlan reason=NoApplyWithoutSelection")
        )));
        assert!(has_trace_value(
            &response,
            "ADAPTER",
            "action",
            "GenerateChangePlan"
        ));
    }

    #[test]
    fn review_validated_plan_preserves_validated_plan() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            prepare_validated_plan(&core);
            let before = core.history.lock().unwrap().current().clone();

            let response = core.execute(request(no_apply_validated_plan_input()));
            let after = response.core_state.expect("state");

            assert_eq!(
                before.session_context.validated_plan,
                after.session_context.validated_plan
            );
            assert!(response.events.iter().any(|event| matches!(
                event,
                CoreEvent::Debug { message }
                    if message.contains("[IR-TRACE][VALIDATED_PLAN_PRESERVE] candidate_id=1")
            )));
        })
    }

    #[test]
    fn review_validated_plan_does_not_clear_selection() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            prepare_validated_plan(&core);

            let response = core.execute(request(no_apply_validated_plan_input()));
            let state = response.core_state.expect("state");

            assert!(state.session_context.selected_candidate.is_some());
            assert!(!response.events.iter().any(|event| matches!(
                event,
                CoreEvent::Debug { message }
                    if message.contains("[IR-TRACE][CONTEXT_CLEAR] reason=NewPlan")
            )));
        })
    }

    #[test]
    fn review_validated_plan_does_not_generate_new_plan() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            prepare_validated_plan(&core);

            let response = core.execute(request(no_apply_validated_plan_input()));

            assert!(!has_trace_value(
                &response,
                "ADAPTER",
                "action",
                "GenerateChangePlan"
            ));
        })
    }

    #[test]
    fn repl_no_apply_with_validated_plan_preserves_context() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            prepare_validated_plan(&core);

            let response = core.execute(request(no_apply_validated_plan_input()));
            let state = response.core_state.expect("state");

            assert!(state.session_context.selected_candidate.is_some());
            assert!(state.session_context.validated_plan.is_some());
            assert!(state.session_context.previous_validation_context.is_some());
        })
    }

    #[test]
    fn repl_no_apply_with_validated_plan_does_not_generate_new_plan() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            prepare_validated_plan(&core);

            let response = core.execute(request(no_apply_validated_plan_input()));

            assert!(!has_trace_value(
                &response,
                "ADAPTER",
                "action",
                "GenerateChangePlan"
            ));
            assert!(!response.events.iter().any(|event| matches!(
                event,
                CoreEvent::Debug { message }
                    if message.contains("[IR-TRACE][CONTEXT_CLEAR] reason=NewPlan")
            )));
        })
    }

    #[test]
    fn repl_no_apply_with_validated_plan_outputs_validation_review() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            prepare_validated_plan(&core);

            let response = core.execute(request(no_apply_validated_plan_input()));

            assert!(response.events.iter().any(|event| matches!(
                event,
                CoreEvent::Result { message }
                    if message.contains("# Validation Review")
                        && message.contains("Apply status: Deferred")
                        && message.contains("No apply executed")
            )));
        })
    }

    #[test]
    fn repl_no_apply_with_validated_plan_does_not_clear_selection() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            prepare_validated_plan(&core);

            let response = core.execute(request(no_apply_validated_plan_input()));
            let state = response.core_state.expect("state");

            assert!(state.session_context.selected_candidate.is_some());
        })
    }

    #[test]
    fn repl_no_apply_with_validated_plan_does_not_apply() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            prepare_validated_plan(&core);

            let response = core.execute(request(no_apply_validated_plan_input()));

            assert!(response.events.iter().any(|event| matches!(
                event,
                CoreEvent::Debug { message }
                    if message.contains("[IR-TRACE][APPLY_DEFERRED] reason=NoApplyConstraint")
            )));
            assert!(!has_trace_value(&response, "ADAPTER", "action", "Apply"));
        })
    }

    #[test]
    fn plan_validation_requires_selected_candidate() {
        let core = RuntimeCoreBridge::with_defaults();
        core.execute(request("このプロジェクトの構造を解析して"));
        core.execute(request(
            "このプロジェクトの構造解析結果をもとに、安全な小規模修正プランを作成して。まだ適用しないで",
        ));

        let response = core.execute(request("この候補を検証して"));

        assert_eq!(response.status, ExecutionStatus::Failed);
    }

    #[test]
    fn plan_validation_allows_low_risk_file_target() {
        let fixture = SafeApplyFixture::new();
        fixture.with_current_dir(|| {
            let core = RuntimeCoreBridge::with_defaults();
            core.execute(request("このプロジェクトの構造を解析して"));
            core.execute(request(
                "このプロジェクトの構造解析結果をもとに、安全な小規模修正プランを作成して。まだ適用しないで",
            ));
            core.execute(request("select 1"));

            let response = core.execute(request("この候補を検証して"));
            let state = response.core_state.expect("core state");

            assert_eq!(response.status, ExecutionStatus::Executed);
            assert!(state.session_context.validated_plan.is_some());
            assert!(
                state
                    .session_context
                    .validated_plan
                    .as_ref()
                    .expect("validated")
                    .apply_allowed
            );
        })
    }

    #[test]
    fn new_analysis_clears_plan_validation_selection() {
        let mut state = CoreState::default();
        state.session_context.previous_plan_context = Some(
            crate::nl::context_aware_plan_target_resolver::PreviousPlanContext {
                target: IrTarget::WorkspaceRoot,
                mode: crate::nl::language_core_ir_adapter::ExecutionMode::PlanOnly,
                candidate_count: 1,
                candidates: vec![ChangePlanCandidate {
                    candidate_id: 1,
                    title: "test".to_string(),
                    target: NarrowTarget::File("Cargo.toml".to_string()),
                    proposed_change: "test".to_string(),
                    rationale: "test".to_string(),
                    risk_level: crate::runtime::autonomous_control::RiskLevel::Low,
                    requires_validation: true,
                }],
                plan_hash: 1,
                status: ExecutionStatus::Executed,
                timestamp: 1,
            },
        );
        state.session_context.selected_candidate = Some(
            crate::nl::context_aware_plan_target_resolver::SelectedCandidateContext {
                candidate_id: 1,
                target: NarrowTarget::File("Cargo.toml".to_string()),
                plan_hash: 1,
                timestamp: 1,
            },
        );

        state.session_context.store_analysis(
            IrAction::AnalyzeProject,
            IrTarget::WorkspaceRoot,
            crate::nl::language_core_ir_adapter::ExecutionMode::ReadOnly,
            ExecutionStatus::Executed,
        );

        assert!(state.session_context.previous_analysis_context.is_some());
        assert!(state.session_context.previous_plan_context.is_none());
        assert!(state.session_context.selected_candidate.is_none());
    }

    #[test]
    fn new_plan_clears_validation_selection() {
        let mut state = CoreState::default();
        state.session_context.selected_candidate = Some(
            crate::nl::context_aware_plan_target_resolver::SelectedCandidateContext {
                candidate_id: 1,
                target: NarrowTarget::File("Cargo.toml".to_string()),
                plan_hash: 1,
                timestamp: 1,
            },
        );

        state.session_context.store_plan(
            IrTarget::WorkspaceRoot,
            crate::nl::language_core_ir_adapter::ExecutionMode::PlanOnly,
            2,
            vec![ChangePlanCandidate {
                candidate_id: 1,
                title: "test".to_string(),
                target: NarrowTarget::File("Cargo.toml".to_string()),
                proposed_change: "test".to_string(),
                rationale: "test".to_string(),
                risk_level: crate::runtime::autonomous_control::RiskLevel::Low,
                requires_validation: true,
            }],
            42,
            ExecutionStatus::Executed,
        );

        assert!(state.session_context.previous_plan_context.is_some());
        assert!(state.session_context.selected_candidate.is_none());
    }

    #[test]
    fn workspace_root_apply_is_rejected_even_with_context() {
        let ir_request = crate::nl::language_core_ir_adapter::IrIntentRequest {
            action: IrAction::Apply,
            target: IrTarget::WorkspaceRoot,
            mode: crate::nl::language_core_ir_adapter::ExecutionMode::Apply,
            raw_input: "apply".to_string(),
            confidence: 1.0,
            safety_constraints: crate::nl::language_core_ir_adapter::SafetyConstraints::default(),
            target_failure: None,
        };

        let rendered = format_change_plan(&ir_request, &[]);

        assert!(rendered.contains("WorkspaceRoot apply prohibited"));
    }

    #[test]
    fn apply_without_validated_plan_is_rejected() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("問題なければ適用して"));

        assert_eq!(response.status, ExecutionStatus::Failed);
    }

    /// spec §11.2 テスト 2: プロジェクト構造解析は WorkspaceRoot をターゲットにする
    ///
    /// Adapter が AnalyzeProject + WorkspaceRoot を設定していることを、
    /// [IR-TRACE][ADAPTER] デバッグイベントから確認する。
    #[test]
    fn nl_project_structure_request_targets_workspace_root() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("このプロジェクトの構造を解析して"));

        let has_adapter_trace = response.events.iter().any(|event| {
            matches!(event, CoreEvent::Debug { message }
                if message.contains("ADAPTER")
                    && message.contains("WorkspaceRoot"))
        });
        assert!(
            has_adapter_trace,
            "ADAPTER trace must include WorkspaceRoot target; events={:?}",
            response
                .events
                .iter()
                .filter(|e| matches!(e, CoreEvent::Debug { .. }))
                .collect::<Vec<_>>()
        );
    }

    /// spec §11.2 テスト 3: プロジェクト構造解析は ReadOnly モードで実行される
    ///
    /// [IR-TRACE][ADAPTER] イベントの mode フィールドが ReadOnly であること。
    #[test]
    fn nl_project_structure_request_is_read_only() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("このプロジェクトの構造を解析して"));

        let has_readonly_trace = response.events.iter().any(|event| {
            matches!(event, CoreEvent::Debug { message }
                if message.contains("ADAPTER")
                    && message.contains("ReadOnly"))
        });
        assert!(
            has_readonly_trace,
            "ADAPTER trace must indicate ReadOnly mode; events={:?}",
            response
                .events
                .iter()
                .filter(|e| matches!(e, CoreEvent::Debug { .. }))
                .collect::<Vec<_>>()
        );
    }

    /// spec §11.2 テスト 4: 依存関係解析要求も WorkspaceRoot をターゲットにする
    ///
    /// 「依存関係を解析して」→ AnalyzeDependencies + WorkspaceRoot + ReadOnly。
    #[test]
    fn nl_dependency_request_targets_workspace_root() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("依存関係を解析して"));

        assert_ne!(
            response.status,
            ExecutionStatus::Failed,
            "Dependency analyze input must not produce Failed status"
        );
        let has_invalid_input_error = response.events.iter().any(
            |event| matches!(event, CoreEvent::Error { message } if message.contains("InvalidInput")),
        );
        assert!(
            !has_invalid_input_error,
            "Response must not contain InvalidInput error for dependency analyze input"
        );
        let has_workspace_root = response.events.iter().any(|event| {
            matches!(event, CoreEvent::Debug { message }
                if message.contains("ADAPTER")
                    && message.contains("WorkspaceRoot"))
        });
        assert!(
            has_workspace_root,
            "Dependency analyze must target WorkspaceRoot"
        );
    }

    /// spec §11.2 テスト 5: リファクタリング要求は即時 apply しない
    ///
    /// 「修正して」→ PlanOnly モード。Apply (ExecutionStatus::Executed で Diff イベント) は出ない。
    /// また RefactorRequest は Adapter を通過した後、通常の NL pipeline で曖昧として
    /// Proposed になるか、あるいは PlanOnly として処理される。
    /// いずれにせよ ReadOnly な analyze と違い apply は走らない。
    #[test]
    fn nl_refactor_request_does_not_apply_immediately() {
        let core = RuntimeCoreBridge::with_defaults();
        // Adapter が is_analyze() = false を返すため通常パスへ進む
        // 「修正して」は ambiguous → Proposed か、PlanOnly で止まる
        let response = core.execute(request("修正して"));

        // Adapter trace で Apply モードになっていないことを確認
        let has_apply_mode = response.events.iter().any(|event| {
            matches!(event, CoreEvent::Debug { message }
                if message.contains("ADAPTER")
                    && message.contains("\"Apply\""))
        });
        assert!(
            !has_apply_mode,
            "Refactor request must not produce Apply mode in ADAPTER trace"
        );
    }
}
// DBM clarification execution guarantee
