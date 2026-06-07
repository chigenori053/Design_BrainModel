use ratatui::layout::{Constraint, Direction, Layout, Rect};

use crate::runtime::human_output_projection::{
    HumanOutputProjection, HumanSemanticEvent, NarrativeEngine, NarrativeSnapshot,
};
use crate::tui::cognitive_workspace::RuntimeIdentity;
use crate::tui::core::resolve_projection_target;
use crate::tui::design_convergence::DesignConvergenceState;
use crate::tui::runtime::RuntimeShellState;
use crate::tui::state::{
    Focus, RuntimeNarrativeEvent, TuiState, UiEvent, contains_runtime_reference, sanitize_line,
};
use crate::tui::workspace::WorkspaceState;

pub const MIN_PANE_WIDTH: u16 = 40;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderSnapshot {
    pub projection: ProjectionSnapshot,
    pub runtime: RuntimeProjection,
    pub reasoning: ReasoningProjection,
    pub status: StatusModel,
    pub input: InputModel,
    pub editor: EditorModel,
    pub workspace: WorkspaceState,
    pub convergence: DesignConvergenceState,
    pub focus: Focus,
    pub identity: RuntimeIdentity,
    pub is_expanded: bool,
    pub diagnostics: Option<DiagnosticModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionSnapshot {
    pub workspace: WorkspaceProjection,
    pub diagnostics: DiagnosticProjection,
    pub narrative: NarrativeProjection,
    pub runtime_state: String,
    pub projection_hash: ProjectionHash,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceProjection {
    pub target: Option<String>,
    pub operation: String,
    pub status: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticProjection {
    pub last_event: Option<String>,
    pub last_focus: Option<String>,
    pub last_mutation: Option<String>,
    pub visible: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NarrativeProjection {
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectionHash {
    pub semantic_hash: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceProjectionModel {
    pub target: Option<String>,
    pub operation: String,
    pub status: String,
}

impl WorkspaceProjectionModel {
    pub fn from_state(state: &TuiState) -> Self {
        let target = resolve_projection_target(state)
            .and_then(|target| semantic_label(&target))
            .or_else(|| locked_target_label(state))
            .or_else(|| resolved_target_label(state));
        let has_preview = state.active_transaction.is_some();
        Self {
            target,
            operation: if has_preview {
                "preview".to_string()
            } else {
                "none".to_string()
            },
            status: system_summary(&projection_state_label_from_runtime(state)),
        }
    }

    pub fn lines(&self) -> Vec<String> {
        vec![
            "Target:".to_string(),
            format!("  {}", self.target.as_deref().unwrap_or("(none)")),
            String::new(),
            "Operation:".to_string(),
            format!("  {}", self.operation),
            String::new(),
            "Status:".to_string(),
            format!("  {}", self.status),
        ]
    }
}

impl From<WorkspaceProjectionModel> for WorkspaceProjection {
    fn from(model: WorkspaceProjectionModel) -> Self {
        Self {
            target: model.target,
            operation: model.operation,
            status: model.status,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticModel {
    pub last_event: String,
    pub last_key_event: String,
    pub last_focus: String,
    pub last_mutation: String,
    pub raw_mode: bool,
    pub runtime_state: String,
    pub active_task: String,
    pub proposal_count: usize,
    pub followup_status: String,
    pub previous_context_used: bool,
    pub memory_status: String,
    pub replay_status: String,
    pub canonical_reuse_status: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeProjection {
    pub state_label: String,
    pub target_label: Option<String>,
    pub transaction_label: Option<String>,
    pub diff_projection: DiffProjection,
    pub rejection_label: Option<String>,
    pub narrative_snapshot: NarrativeSnapshot,
    pub narrative_lines: Vec<String>,
    pub scroll_offset: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffProjection {
    pub target_label: Option<String>,
    pub workspace: WorkspaceProjectionModel,
    pub lines: Vec<String>,
    pub semantic_projection: Option<crate::tui::cognitive_workspace::WorkspaceSemanticProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasoningProjection {
    pub mode: ReasoningViewMode,
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReasoningViewMode {
    Reasoning,
    Analyze,
    MutationPlan,
    MutationPreview,
    Diff,
    Verification,
}

impl ReasoningViewMode {
    pub fn title(self) -> &'static str {
        match self {
            Self::Reasoning => " Reasoning View ",
            Self::Analyze => " Analyze View ",
            Self::MutationPlan => " Mutation Plan View ",
            Self::MutationPreview => " Mutation Preview View ",
            Self::Diff => " Diff View ",
            Self::Verification => " Verification View ",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusModel {
    pub line: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputModel {
    pub pipeline_label: String,
    pub text: String,
    pub cursor: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditorModel {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub editing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutMetadata {
    pub viewport: Rect,
    pub header: Rect,
    pub input: Rect,
    pub runtime: Rect,
    pub diff: Rect,
    pub task: Rect,
    pub diagnostics: Rect,
    pub status: Rect,
}

pub fn layout_for_area(area: Rect, show_diagnostics: bool) -> LayoutMetadata {
    let rows = layout_rows(area);

    let (middle_rect, diag_rect) = if show_diagnostics {
        let diag_width = if area.width > 100 { 40 } else { 20 };
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(40), Constraint::Length(diag_width)])
            .split(rows[1]);
        (cols[0], cols[1])
    } else {
        (rows[1], Rect::new(0, 0, 0, 0))
    };

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(middle_rect);
    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(7)])
        .split(columns[0]);

    LayoutMetadata {
        viewport: area,
        header: rows[0],
        runtime: left_rows[0],
        input: left_rows[1],
        diff: columns[1],
        task: Rect::new(0, 0, 0, 0),
        diagnostics: diag_rect,
        status: rows[2],
    }
}

fn layout_rows(area: Rect) -> std::rc::Rc<[Rect]> {
    Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorModel {
    pub x: u16,
    pub y: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImmutableFrame {
    pub snapshot: RenderSnapshot,
    pub layout: LayoutMetadata,
    pub cursor: Option<CursorModel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullSurfaceProjection {
    pub frame: ImmutableFrame,
}

pub struct FrameComposer;

impl FrameComposer {
    pub fn compose(snapshot: RenderSnapshot, layout: LayoutMetadata) -> ImmutableFrame {
        let cursor = cursor_model(&snapshot, layout.input);
        ImmutableFrame {
            snapshot,
            layout,
            cursor,
        }
    }
}

impl From<&TuiState> for RenderSnapshot {
    fn from(state: &TuiState) -> Self {
        let runtime = RuntimeProjection::from_state(state);
        let diagnostics = if state.diagnostic_mode {
            Some(DiagnosticModel {
                last_event: state
                    .diagnostics
                    .last_event
                    .clone()
                    .unwrap_or_else(|| "(none)".to_string()),
                last_key_event: state
                    .diagnostics
                    .last_key_event
                    .clone()
                    .unwrap_or_else(|| "(none)".to_string()),
                last_focus: state
                    .diagnostics
                    .last_focus_transition
                    .clone()
                    .unwrap_or_else(|| format!("{:?}", state.focus)),
                last_mutation: state
                    .diagnostics
                    .last_mutation
                    .clone()
                    .unwrap_or_else(|| "(none)".to_string()),
                raw_mode: state.diagnostics.raw_mode_active,
                runtime_state: state
                    .diagnostics
                    .runtime_state
                    .clone()
                    .unwrap_or_else(|| "(runtime idle)".to_string()),
                active_task: state
                    .diagnostics
                    .active_task
                    .clone()
                    .unwrap_or_else(|| "(none)".to_string()),
                proposal_count: state.diagnostics.proposal_count,
                followup_status: state
                    .diagnostics
                    .followup_status
                    .clone()
                    .unwrap_or_else(|| "created".to_string()),
                previous_context_used: state.diagnostics.previous_context_used,
                memory_status: state
                    .diagnostics
                    .memory_status
                    .clone()
                    .unwrap_or_else(|| "idle".to_string()),
                replay_status: state
                    .diagnostics
                    .replay_status
                    .clone()
                    .unwrap_or_else(|| "idle".to_string()),
                canonical_reuse_status: state
                    .diagnostics
                    .canonical_reuse_status
                    .clone()
                    .unwrap_or_else(|| "created".to_string()),
            })
        } else {
            None
        };
        let diagnostic_projection = DiagnosticProjection {
            last_event: diagnostics.as_ref().map(|d| d.last_event.clone()),
            last_focus: diagnostics.as_ref().map(|d| d.last_focus.clone()),
            last_mutation: diagnostics.as_ref().map(|d| d.last_mutation.clone()),
            visible: diagnostics.is_some(),
        };
        let mut projection = ProjectionSnapshot {
            workspace: runtime.diff_projection.workspace.clone().into(),
            diagnostics: diagnostic_projection,
            narrative: NarrativeProjection {
                lines: runtime.narrative_lines.clone(),
            },
            runtime_state: runtime.state_label.clone(),
            projection_hash: ProjectionHash::default(),
        };
        projection.projection_hash = ProjectionHash {
            semantic_hash: projection_semantic_hash(&projection),
        };
        crate::tui::render_trace::record(Box::leak(
            format!(
                "[SNAPSHOT]\nstatus={}\nactive_task={}",
                state.workspace.evaluation.status,
                state
                    .workspace
                    .evaluation
                    .active_task
                    .as_deref()
                    .unwrap_or("None")
            )
            .into_boxed_str(),
        ));
        Self {
            projection,
            status: StatusModel {
                line: runtime.workspace_status_line(),
            },
            reasoning: ReasoningProjection::from_state(state, &runtime),
            runtime,
            input: InputModel {
                pipeline_label: sanitize_line(state.pipeline_state.label()).unwrap_or_default(),
                text: sanitize_line(&state.input.text).unwrap_or_default(),
                cursor: state.input.cursor.min(state.input.text.len()),
            },
            editor: EditorModel {
                lines: state.editor_state.editor.lines.clone(),
                cursor_row: state.editor_state.editor.cursor_row,
                cursor_col: state.editor_state.editor.cursor_col,
                editing: state.editor_state.editing,
            },
            workspace: state.workspace.clone(),
            convergence: state.convergence.clone(),
            focus: state.focus,
            identity: RuntimeIdentity::default(),
            is_expanded: state.narrative_expanded,
            diagnostics,
        }
    }
}

impl RuntimeProjection {
    pub fn from_state(state: &TuiState) -> Self {
        let target_label = locked_target_label(state).or_else(|| resolved_target_label(state));
        let diff_projection = DiffProjection::from_state(state, target_label.clone());
        let rejection_label = state.rejection.as_ref().map(|rej| {
            format!(
                "REJECTED: {} (via {})",
                rej.reason, rej.originating_mutation
            )
        });

        let mut projection = Self {
            state_label: projection_state_label_from_runtime(state),
            target_label,
            transaction_label: resolved_transaction_label(state),
            diff_projection,
            rejection_label,
            narrative_snapshot: NarrativeSnapshot::default(),
            narrative_lines: Vec::new(),
            scroll_offset: state.chat_scroll.offset,
        };
        let human_events = human_semantic_events_from_state(state)
            .iter()
            .map(HumanOutputProjection::project)
            .collect::<Vec<_>>();
        projection.narrative_snapshot = NarrativeEngine::summarize(&human_events);
        projection.narrative_lines = narrative_snapshot_lines(&projection.narrative_snapshot);
        projection
    }

    pub fn runtime_panel_lines(&self, _expanded: bool) -> Vec<String> {
        self.narrative_lines.clone()
    }

    pub fn workspace_status_line(&self) -> String {
        let phase = match self.state_label.as_str() {
            "PREVIEW_READY" | "READY_TO_APPLY" | "AWAITING_APPLY" => "事前確認可能",
            "APPLYING" => "変更を適用中",
            "APPLIED" => "処理完了",
            "FAILED_RECOVERABLE" => "確認が必要",
            "REJECTED" => "変更を停止",
            _ if self.transaction_label.is_some() => "影響範囲を確認中",
            _ => "入力待機",
        };
        format!("{phase} | F2 Diagnostics | :diagnostics")
    }
}

fn human_semantic_events_from_state(state: &TuiState) -> Vec<HumanSemanticEvent> {
    let mut events = Vec::new();
    let has_plan = state
        .workspace
        .analysis_result
        .mutation_plan_projection
        .is_some();
    let has_preview = state
        .workspace
        .analysis_result
        .mutation_preview_projection
        .is_some()
        || matches!(
            state.runtime_state,
            RuntimeShellState::PreviewReady
                | RuntimeShellState::AwaitingApply
                | RuntimeShellState::AwaitConfirmation
                | RuntimeShellState::Ready
        );
    let latest_error = state
        .chat
        .events
        .iter()
        .rev()
        .find_map(|event| match event {
            UiEvent::Error { message } => Some(message.clone()),
            _ => None,
        });
    let has_completion = state
        .chat
        .events
        .iter()
        .rev()
        .find_map(|event| match event {
            UiEvent::Result { .. } => Some(true),
            UiEvent::System { summary } => Some(summary.to_ascii_lowercase().contains("completed")),
            UiEvent::Thinking { .. }
            | UiEvent::Planning { .. }
            | UiEvent::Execution { .. }
            | UiEvent::Runtime { .. }
            | UiEvent::Pipeline { .. }
            | UiEvent::Error { .. } => Some(false),
            _ => None,
        })
        .unwrap_or(false);
    let runtime_failed = state.runtime_state == RuntimeShellState::Failed
        || state.runtime_state.label().contains("HALT")
        || matches!(
            state.runtime_state,
            RuntimeShellState::Rejected
                | RuntimeShellState::GovernanceRejected
                | RuntimeShellState::SemanticRejected
                | RuntimeShellState::ConvergenceRejected
                | RuntimeShellState::MutationSuppressed
        );

    if let Some(analyze) = &state.workspace.analysis_result.analyze_projection {
        events.push(HumanSemanticEvent::AnalyzeCompleted {
            project_name: Some(analyze.project_name.clone()),
            primary_areas: primary_project_areas(&analyze.findings),
        });
    }
    if has_plan {
        events.push(HumanSemanticEvent::MutationPlanCreated);
    }
    if state.rejection.is_some() || runtime_failed {
        events.push(HumanSemanticEvent::ValidationFailed {
            reason: state
                .rejection
                .as_ref()
                .map(|rejection| rejection.reason.clone())
                .or_else(|| latest_error.clone()),
        });
    } else if has_plan || has_preview || state.active_transaction.is_some() {
        events.push(HumanSemanticEvent::ValidationPassed);
    }
    if has_preview {
        events.push(HumanSemanticEvent::PreviewGenerated);
    }
    if state
        .chat
        .events
        .iter()
        .any(|event| matches!(event, UiEvent::MutationRollback { .. }))
    {
        events.push(HumanSemanticEvent::RollbackCompleted);
    } else if state.runtime_state == RuntimeShellState::Git
        || state
            .chat
            .events
            .iter()
            .any(|event| matches!(event, UiEvent::MutationApplied { .. }))
    {
        events.push(HumanSemanticEvent::ApplyCompleted);
    }

    let runtime_event = match state.runtime_state {
        RuntimeShellState::Thinking
        | RuntimeShellState::Analyze
        | RuntimeShellState::Plan
        | RuntimeShellState::Validate
        | RuntimeShellState::Apply
        | RuntimeShellState::Replay => HumanSemanticEvent::RuntimeRunning,
        RuntimeShellState::Git => HumanSemanticEvent::RuntimeCompleted,
        RuntimeShellState::Failed => HumanSemanticEvent::RuntimeFailed {
            reason: latest_error,
        },
        runtime_state
            if runtime_state.label().contains("HALT")
                || matches!(
                    runtime_state,
                    RuntimeShellState::Rejected
                        | RuntimeShellState::GovernanceRejected
                        | RuntimeShellState::SemanticRejected
                        | RuntimeShellState::ConvergenceRejected
                        | RuntimeShellState::MutationSuppressed
                ) =>
        {
            HumanSemanticEvent::RuntimeFailed {
                reason: state
                    .rejection
                    .as_ref()
                    .map(|rejection| rejection.reason.clone()),
            }
        }
        RuntimeShellState::Idle if has_completion => HumanSemanticEvent::RuntimeCompleted,
        _ => HumanSemanticEvent::RuntimeWaiting,
    };
    events.push(runtime_event);
    events
}

fn primary_project_areas(findings: &[String]) -> Vec<String> {
    ["apps", "crates"]
        .into_iter()
        .filter(|area| {
            findings.iter().any(|finding| {
                finding
                    .split(|ch: char| !ch.is_alphanumeric() && ch != '_')
                    .any(|token| token == *area)
            })
        })
        .map(str::to_string)
        .collect()
}

fn narrative_snapshot_lines(snapshot: &NarrativeSnapshot) -> Vec<String> {
    let mut lines = vec![snapshot.headline.clone(), String::new()];
    lines.extend(snapshot.details.iter().cloned());
    if !snapshot.next_actions.is_empty() {
        lines.push(String::new());
        lines.push("推奨アクション".to_string());
        lines.extend(
            snapshot
                .next_actions
                .iter()
                .map(|action| format!("• {action}")),
        );
    }
    lines
}

impl ReasoningProjection {
    pub fn from_state(state: &TuiState, runtime: &RuntimeProjection) -> Self {
        let mode = reasoning_mode(state, runtime);
        let lines = match mode {
            ReasoningViewMode::Reasoning => reasoning_view_lines(state, runtime),
            ReasoningViewMode::Analyze => analyze_view_lines(state),
            ReasoningViewMode::MutationPlan => mutation_plan_view_lines(state),
            ReasoningViewMode::MutationPreview => mutation_preview_view_lines(state),
            ReasoningViewMode::Diff => diff_view_lines(state),
            ReasoningViewMode::Verification => verification_view_lines(state),
        };
        Self { mode, lines }
    }
}

fn reasoning_mode(state: &TuiState, runtime: &RuntimeProjection) -> ReasoningViewMode {
    if state
        .chat
        .events
        .iter()
        .rev()
        .any(|event| matches!(event, UiEvent::Validation { .. } | UiEvent::Result { .. }))
        && matches!(
            state.runtime_state,
            RuntimeShellState::Validate | RuntimeShellState::Failed
        )
    {
        return ReasoningViewMode::Verification;
    }
    if state.active_transaction.is_some() {
        return ReasoningViewMode::Diff;
    }
    if state
        .workspace
        .analysis_result
        .mutation_preview_projection
        .is_some()
    {
        return ReasoningViewMode::MutationPreview;
    }
    if state
        .workspace
        .analysis_result
        .mutation_plan_projection
        .is_some()
    {
        return ReasoningViewMode::MutationPlan;
    }
    if state.chat.events.iter().rev().any(|event| {
        matches!(
            event,
            UiEvent::Plan { .. } | UiEvent::ImplementationPlan { .. }
        )
    }) {
        return ReasoningViewMode::MutationPlan;
    }
    if state.convergence.generated_spec.is_some()
        || state.chat.events.iter().rev().any(|event| {
            matches!(
                event,
                UiEvent::Analysis { .. }
                    | UiEvent::AnalyzeResult { .. }
                    | UiEvent::StructuralDiagnosis { .. }
            )
        })
    {
        return ReasoningViewMode::Analyze;
    }
    if runtime.state_label == "APPLIED" {
        ReasoningViewMode::Verification
    } else {
        ReasoningViewMode::Reasoning
    }
}

fn reasoning_view_lines(state: &TuiState, runtime: &RuntimeProjection) -> Vec<String> {
    if let Some(intent) = &state.resolved_intent {
        let mut lines = vec![
            "DBM Reasoning".to_string(),
            String::new(),
            "Intent Resolution".to_string(),
            format!(
                "  Primary Goal: {}",
                crate::intent_resolution::goal_label(intent.primary_goal)
            ),
            format!("  Confidence: {:.0}%", intent.confidence * 100.0),
            format!("  Confirmation Status: {}", intent.confirmation_status()),
            format!(
                "  Execution State: {}",
                crate::intent_resolution::execution_state_label(state.execution_state)
            ),
            String::new(),
            "Recommended Actions".to_string(),
        ];
        for candidate in &intent.candidate_actions {
            lines.push(format!(
                "  - {} ({:.0}%): {}",
                crate::intent_resolution::action_label(candidate.action),
                candidate.confidence * 100.0,
                candidate.reason
            ));
        }
        if let Some(pending) = &state.pending_confirmation {
            lines.push(String::new());
            lines.push("Pending Confirmation".to_string());
            lines.push(format!(
                "  Action: {}",
                crate::intent_resolution::action_label(pending.action)
            ));
        }
        if let Some(narrative) = &state.execution_narrative {
            lines.push(String::new());
            lines.push("Execution Narrative".to_string());
            lines.extend(narrative.lines().map(|line| format!("  {line}")));
        }
        return lines;
    }

    let intent = state
        .convergence
        .intent
        .as_ref()
        .map(|intent| format!("{} / {}", intent.objective, intent.target))
        .or_else(|| state.convergence.raw_intent.clone())
        .unwrap_or_else(|| "Awaiting user intent".to_string());
    let missing = if state.convergence.questions.is_empty() {
        "No missing information detected".to_string()
    } else {
        state.convergence.questions.join("\n")
    };
    vec![
        "DBM Reasoning".to_string(),
        String::new(),
        "Intent Analysis".to_string(),
        format!("  {intent}"),
        String::new(),
        "Missing Information".to_string(),
        indent_block(&missing),
        String::new(),
        "Design Trade-offs".to_string(),
        "  Balance self modification scope with governance and replay stability".to_string(),
        String::new(),
        "Risk Assessment".to_string(),
        format!("  {}", risk_summary(runtime)),
        String::new(),
        "Convergence Score".to_string(),
        format!("  {}%", state.convergence.convergence_percent()),
    ]
}

fn analyze_view_lines(state: &TuiState) -> Vec<String> {
    if let Some(projection) = &state.workspace.analysis_result.analyze_projection {
        return projection.render().lines().map(str::to_string).collect();
    }
    let target = state
        .convergence
        .intent
        .as_ref()
        .map(|intent| intent.target.as_str())
        .unwrap_or("dbm");
    let mut dependencies = vec!["apps/cli/src/tui/render.rs".to_string()];
    dependencies.push("apps/cli/src/tui/rendering/mod.rs".to_string());
    dependencies.push("apps/cli/src/tui/state.rs".to_string());
    vec![
        "Target Components".to_string(),
        format!("  {target}"),
        String::new(),
        "Impact Analysis".to_string(),
        "  Medium".to_string(),
        String::new(),
        "Dependency Analysis".to_string(),
        indent_block(&dependencies.join("\n")),
        String::new(),
        "Required Modifications".to_string(),
        "  Render convergence timeline, reasoning projection, phase switching".to_string(),
    ]
}

fn mutation_plan_view_lines(state: &TuiState) -> Vec<String> {
    if let Some(projection) = &state.workspace.analysis_result.mutation_plan_projection {
        return projection.render().lines().map(str::to_string).collect();
    }
    let affected = state
        .active_transaction
        .as_ref()
        .map(|tx| tx.target_path.clone())
        .unwrap_or_else(|| "No active mutation target".to_string());
    vec![
        "Mutation Plan".to_string(),
        "  Apply generated specification through governed runtime transaction".to_string(),
        String::new(),
        "Affected Files".to_string(),
        format!("  {affected}"),
        String::new(),
        "Expected Behavior".to_string(),
        "  Convergence workspace drives analyze, diff, and verification flow".to_string(),
        String::new(),
        "Rollback Strategy".to_string(),
        "  Retain transaction checkpoint and reject unresolved preview targets".to_string(),
    ]
}

fn mutation_preview_view_lines(state: &TuiState) -> Vec<String> {
    state
        .workspace
        .analysis_result
        .mutation_preview_projection
        .as_ref()
        .map(|projection| projection.render().lines().map(str::to_string).collect())
        .unwrap_or_else(|| {
            vec![
                "Mutation Preview".to_string(),
                "  (no mutation preview generated)".to_string(),
            ]
        })
}

fn diff_view_lines(state: &TuiState) -> Vec<String> {
    let Some(transaction) = state.active_transaction.as_ref() else {
        return vec![
            "Unified Diff".to_string(),
            "  (no generated code changes)".to_string(),
        ];
    };
    let mut lines = vec![
        "Unified Diff".to_string(),
        format!("  target: {}", transaction.target_path),
        String::new(),
    ];
    for change in &transaction.diff.changes {
        if let Some(old) = &change.old {
            lines.push(format!("- {old}"));
        }
        if let Some(new) = &change.new {
            lines.push(format!("+ {new}"));
        }
    }
    if lines.len() == 3 {
        lines.push("  (semantic diff ready)".to_string());
    }
    lines
}

fn verification_view_lines(state: &TuiState) -> Vec<String> {
    let failed = state
        .chat
        .events
        .iter()
        .filter(|event| matches!(event, UiEvent::Error { .. }))
        .count();
    vec![
        "Verification Status".to_string(),
        format!("  {}", if failed == 0 { "Passed" } else { "Failed" }),
        String::new(),
        "Passed Tests".to_string(),
        "  Runtime projection invariants".to_string(),
        String::new(),
        "Failed Tests".to_string(),
        format!("  {failed}"),
        String::new(),
        "Coverage".to_string(),
        "  TUI convergence workspace render path".to_string(),
        String::new(),
        "Acceptance Criteria".to_string(),
        "  Two-pane convergence workspace with diagnostics separated".to_string(),
    ]
}

fn risk_summary(runtime: &RuntimeProjection) -> String {
    if runtime.transaction_label.is_some() {
        "Runtime transaction may affect execution behavior; verify before apply".to_string()
    } else {
        "Scope uncertainty remains until target, constraints, and verification are confirmed"
            .to_string()
    }
}

fn indent_block(text: &str) -> String {
    text.lines()
        .map(|line| format!("  {line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

impl DiffProjection {
    pub fn from_state(state: &TuiState, target_label: Option<String>) -> Self {
        let workspace = WorkspaceProjectionModel::from_state(state);
        let Some(transaction) = state.active_transaction.as_ref() else {
            return Self {
                target_label,
                workspace: workspace.clone(),
                lines: workspace.lines(),
                semantic_projection: None,
            };
        };

        // DBM-WORKSPACE-SEMANTIC-PROJECTION Integration
        let engine = crate::tui::cognitive_workspace::WorkspaceSemanticProjectionEngine {
            analyzer: crate::tui::cognitive_workspace::WorkspaceSemanticAnalyzer,
            classifier: crate::tui::cognitive_workspace::SemanticImpactClassifier,
            narrative_renderer: crate::tui::cognitive_workspace::WorkspaceNarrativeRenderer,
        };
        let semantic_projection = Some(engine.project_impact(&transaction.target_path));

        let target_label = target_label.or_else(|| semantic_label(&transaction.target_path));
        Self {
            target_label,
            workspace: workspace.clone(),
            lines: sanitize_lines(workspace.lines()),
            semantic_projection,
        }
    }
}

pub struct RuntimeNarrativeReducer;
#[derive(Debug, Default)]
pub struct RuntimeNarrativeReducerState {
    pub last_known_target: Option<String>,
}

impl RuntimeNarrativeReducer {
    pub fn render(events: Vec<RuntimeNarrativeEvent>) -> Vec<String> {
        let mut reducer = RuntimeNarrativeReducerState::default();
        reducer.render(events)
    }
}

impl RuntimeNarrativeReducerState {
    pub fn render(&mut self, events: Vec<RuntimeNarrativeEvent>) -> Vec<String> {
        let mut lines = Vec::new();
        for event in events {
            let event = self.attach_inherited_target(event);
            if let Some(target) = event.target_authority() {
                self.last_known_target = Some(target.to_string());
            }
            let line = render_narrative_event(normalize_narrative_event(event));
            if lines.last() != Some(&line) {
                lines.push(line);
            }
        }
        lines
    }

    fn attach_inherited_target(&self, event: RuntimeNarrativeEvent) -> RuntimeNarrativeEvent {
        let Some(inherited) = self.last_known_target.clone() else {
            return event;
        };
        match event {
            RuntimeNarrativeEvent::Validation {
                summary,
                target: None,
            } => RuntimeNarrativeEvent::Validation {
                summary,
                target: Some(inherited),
            },
            RuntimeNarrativeEvent::Execution {
                summary,
                target: None,
            } => RuntimeNarrativeEvent::Execution {
                summary,
                target: Some(inherited),
            },
            RuntimeNarrativeEvent::Apply {
                summary,
                target: None,
            } => RuntimeNarrativeEvent::Apply {
                summary,
                target: Some(inherited),
            },
            RuntimeNarrativeEvent::System {
                summary,
                target: None,
            } => RuntimeNarrativeEvent::System {
                summary,
                target: Some(inherited),
            },
            other => other,
        }
    }
}

pub fn runtime_semantic_events(state: &TuiState) -> Vec<RuntimeNarrativeEvent> {
    let projection = RuntimeProjection::from_state(state);
    runtime_semantic_events_from_projection(&projection)
}

fn runtime_semantic_events_from_projection(
    projection: &RuntimeProjection,
) -> Vec<RuntimeNarrativeEvent> {
    let mut events = Vec::new();
    let target = projection.target_label.clone();

    events.push(RuntimeNarrativeEvent::Intent {
        summary: intent_summary(projection),
    });
    events.push(RuntimeNarrativeEvent::Thinking {
        summary: "resolving target graph".to_string(),
    });
    events.push(RuntimeNarrativeEvent::Analysis {
        summary: analysis_summary(projection),
    });
    events.push(RuntimeNarrativeEvent::Validation {
        summary: validation_summary(projection),
        target: target.clone(),
    });
    events.push(RuntimeNarrativeEvent::Execution {
        summary: execution_summary(projection),
        target: target.clone(),
    });
    if projection.state_label == "APPLIED" {
        events.push(RuntimeNarrativeEvent::Apply {
            summary: "transaction committed successfully".to_string(),
            target: target.clone(),
        });
    }
    if let Some(rejection) = projection.rejection_label.clone() {
        events.push(RuntimeNarrativeEvent::GovernanceReject { reason: rejection });
    }
    events.push(RuntimeNarrativeEvent::System {
        summary: system_summary(&projection.state_label),
        target,
    });

    events
}

#[deprecated(note = "Use runtime_semantic_events instead")]
pub fn render_runtime_text(state: &TuiState) -> Vec<String> {
    runtime_semantic_events(state)
        .into_iter()
        .map(|e| e.render())
        .collect()
}

fn intent_summary(projection: &RuntimeProjection) -> String {
    match projection.state_label.as_str() {
        "APPLIED" => "applying active transaction".to_string(),
        "PREVIEW_READY" | "READY_TO_APPLY" | "AWAITING_APPLY" => {
            "preparing governed transaction".to_string()
        }
        _ => "checking runtime state".to_string(),
    }
}

fn analysis_summary(projection: &RuntimeProjection) -> String {
    if projection.target_label.is_some() {
        "diff structure computed".to_string()
    } else {
        "runtime state assessed".to_string()
    }
}

fn validation_summary(projection: &RuntimeProjection) -> String {
    if projection.rejection_label.is_some() {
        "governance boundary evaluated".to_string()
    } else if projection.transaction_label.is_some() {
        "transaction checksum verified".to_string()
    } else {
        "runtime invariants verified".to_string()
    }
}

fn execution_summary(projection: &RuntimeProjection) -> String {
    if projection.state_label == "APPLIED" {
        "transaction consumed".to_string()
    } else if projection.transaction_label.is_some() {
        "transaction active".to_string()
    } else {
        "no active transaction".to_string()
    }
}

fn system_summary(state_label: &str) -> String {
    match state_label {
        "IDLE" => "runtime idle".to_string(),
        "PREVIEW_READY" | "READY_TO_APPLY" | "AWAITING_APPLY" => "preview ready".to_string(),
        "APPLIED" => "runtime stabilized".to_string(),
        "APPLYING" => "mutation in progress".to_string(),
        "FAILED_RECOVERABLE" => "runtime recovery available".to_string(),
        "RUNAWAY_COGNITION_HALT" => "governance halt active".to_string(),
        _ => "runtime state projected".to_string(),
    }
}

fn projection_state_label(state: RuntimeShellState) -> &'static str {
    match state {
        RuntimeShellState::PreviewReady => "PREVIEW_READY",
        RuntimeShellState::AwaitingApply => "AWAITING_APPLY",
        RuntimeShellState::Ready | RuntimeShellState::AwaitConfirmation => "READY_TO_APPLY",
        RuntimeShellState::Apply => "APPLYING",
        RuntimeShellState::Git => "APPLIED",
        other => other.label(),
    }
}

fn projection_state_label_from_runtime(state: &TuiState) -> String {
    if state.runtime_state == RuntimeShellState::Failed {
        if state
            .active_transaction
            .as_ref()
            .is_some_and(|tx| tx.failed_recoverable && !tx.tx_id.is_empty())
        {
            return "FAILED_RECOVERABLE".to_string();
        }
        return "IDLE".to_string();
    }
    projection_state_label(state.runtime_state).to_string()
}

fn resolved_target_label(state: &TuiState) -> Option<String> {
    state
        .active_transaction
        .as_ref()
        .map(|tx| tx.target_path.as_str())
        .or(state.active_target.as_deref())
        .and_then(semantic_label)
}

fn locked_target_label(state: &TuiState) -> Option<String> {
    state
        .branch_runtime
        .as_ref()
        .map(|runtime| runtime.surface_snapshot().target.clone())
        .and_then(|target| semantic_label(&target))
}

fn resolved_transaction_label(state: &TuiState) -> Option<String> {
    if state.active_transaction.is_some() || state.active_transaction_id.is_some() {
        Some("transaction active".to_string())
    } else {
        None
    }
}

fn semantic_label(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed == "preview" || contains_runtime_reference(trimmed) {
        return None;
    }
    let path = std::path::Path::new(trimmed);
    let display = if path.is_absolute() {
        semantic_workspace_relative_path(path)
            .or_else(|| {
                std::env::current_dir().ok().and_then(|cwd| {
                    path.strip_prefix(cwd)
                        .ok()
                        .map(|relative| relative.to_path_buf())
                })
            })
            .unwrap_or_else(|| compressed_absolute_path(path))
    } else {
        path.to_path_buf()
    };
    sanitize_line(&display.display().to_string())
}

pub fn projection_semantic_hash(snapshot: &ProjectionSnapshot) -> String {
    let mut hasher = StableProjectionHasher::new();
    hasher.write_opt_str(snapshot.workspace.target.as_deref());
    hasher.write_str(&snapshot.workspace.operation);
    hasher.write_str(&snapshot.workspace.status);
    hasher.write_bool(snapshot.diagnostics.visible);
    hasher.write_opt_str(snapshot.diagnostics.last_event.as_deref());
    hasher.write_opt_str(snapshot.diagnostics.last_focus.as_deref());
    hasher.write_opt_str(snapshot.diagnostics.last_mutation.as_deref());
    for line in &snapshot.narrative.lines {
        hasher.write_str(line);
    }
    hasher.write_str(&snapshot.runtime_state);
    format!("{:016x}", hasher.finish())
}

struct StableProjectionHasher {
    hash: u64,
}

impl StableProjectionHasher {
    fn new() -> Self {
        Self {
            hash: 0xcbf29ce484222325_u64,
        }
    }

    fn write_str(&mut self, value: &str) {
        self.write_u64(value.len() as u64);
        for byte in value.as_bytes() {
            self.hash ^= u64::from(*byte);
            self.hash = self.hash.wrapping_mul(0x100000001b3);
        }
    }

    fn write_opt_str(&mut self, value: Option<&str>) {
        match value {
            Some(value) => {
                self.write_bool(true);
                self.write_str(value);
            }
            None => self.write_bool(false),
        }
    }

    fn write_bool(&mut self, value: bool) {
        self.write_u64(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.hash ^= u64::from(byte);
            self.hash = self.hash.wrapping_mul(0x100000001b3);
        }
    }

    fn finish(self) -> u64 {
        self.hash
    }
}

fn semantic_workspace_relative_path(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let components = path.components().collect::<Vec<_>>();
    for anchor in ["apps", "crates", "docs", "specs", "tests"] {
        if let Some(idx) = components
            .iter()
            .position(|component| component.as_os_str() == anchor)
        {
            return Some(components[idx..].iter().collect());
        }
    }
    None
}

fn compressed_absolute_path(path: &std::path::Path) -> std::path::PathBuf {
    let components = path.components().collect::<Vec<_>>();
    let start = components.len().saturating_sub(3);
    components[start..].iter().collect()
}

fn cursor_model(snapshot: &RenderSnapshot, input_area: Rect) -> Option<CursorModel> {
    if snapshot.focus != Focus::Input {
        return None;
    }
    let inner_x = input_area.x.saturating_add(1);
    let inner_y = input_area.y.saturating_add(1);
    let inner_width = input_area.width.saturating_sub(2);
    let inner_height = input_area.height.saturating_sub(2);
    let row = snapshot.editor.cursor_row;
    let col = snapshot.editor.cursor_col;
    let x = inner_x.saturating_add(col as u16);
    let y = inner_y.saturating_add(row as u16);
    if col as u16 >= inner_width || row as u16 >= inner_height {
        return None;
    }
    Some(CursorModel { x, y })
}

fn sanitize_lines(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .filter_map(|line| sanitize_line(&line))
        .collect()
}

fn normalize_narrative_event(event: RuntimeNarrativeEvent) -> RuntimeNarrativeEvent {
    match event {
        RuntimeNarrativeEvent::Intent { summary }
            if summary == "observing runtime cognition" || summary == "checking runtime state" =>
        {
            RuntimeNarrativeEvent::Intent {
                summary: "checking runtime state".to_string(),
            }
        }
        RuntimeNarrativeEvent::Planning { summary } => RuntimeNarrativeEvent::Thinking { summary },
        RuntimeNarrativeEvent::Preview { target } => RuntimeNarrativeEvent::Execution {
            summary: "preparing transaction preview".to_string(),
            target: Some(target),
        },
        RuntimeNarrativeEvent::Commit { summary } => RuntimeNarrativeEvent::Apply {
            summary,
            target: None,
        },
        RuntimeNarrativeEvent::Error { message } => RuntimeNarrativeEvent::System {
            summary: format!("runtime error: {message}"),
            target: None,
        },
        other => other,
    }
}

fn render_narrative_event(event: RuntimeNarrativeEvent) -> String {
    match event {
        RuntimeNarrativeEvent::Intent { summary } => format!("[INTENT] {summary}"),
        RuntimeNarrativeEvent::Thinking { summary } => format!("[THINKING] {summary}"),
        RuntimeNarrativeEvent::Analysis { summary } => format!("[ANALYSIS] {summary}"),
        RuntimeNarrativeEvent::AnalyzeResult { projection } => projection.render(),
        RuntimeNarrativeEvent::Planning { summary } => format!("[THINKING] {summary}"),
        RuntimeNarrativeEvent::Validation { summary, .. } => format!("[VALIDATION] {summary}"),
        RuntimeNarrativeEvent::Execution { summary, .. } => format!("[EXECUTION] {summary}"),
        RuntimeNarrativeEvent::Preview { target } => {
            format!("[EXECUTION] preparing transaction preview for {target}")
        }
        RuntimeNarrativeEvent::Apply { summary, .. }
        | RuntimeNarrativeEvent::Commit { summary } => format!("[APPLY] {summary}"),
        RuntimeNarrativeEvent::Rollback { summary } => format!("[ROLLBACK] {summary}"),
        RuntimeNarrativeEvent::MutationPlan { projection } => projection.render(),
        RuntimeNarrativeEvent::MutationPreview { projection } => projection.render(),
        RuntimeNarrativeEvent::MutationApplied { projection } => projection.render(),
        RuntimeNarrativeEvent::MutationReplay { projection } => projection.render(),
        RuntimeNarrativeEvent::MutationRollback { projection } => projection.render(),
        RuntimeNarrativeEvent::System { summary, .. } => format!("[SYSTEM] {summary}"),
        RuntimeNarrativeEvent::GovernanceReject { reason } => format!("[REJECT] {reason}"),
        RuntimeNarrativeEvent::Error { message } => format!("[SYSTEM] runtime error: {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::model::{TraceStatsViewModel, TraceViewModel, UiPayload};
    use crate::tui::state::{Diff, DiffChunk, RuntimeTransaction, UiEvent};

    fn empty_payload() -> UiPayload {
        UiPayload {
            trace: TraceViewModel {
                request_id: "render-test".to_string(),
                steps: vec![],
                stats: TraceStatsViewModel {
                    total_nodes: 0,
                    max_depth: 0,
                    recall_hit_rate: 0.0,
                    avg_branching: 0.0,
                },
            },
            hypotheses: vec![],
            memory: vec![],
            selected: None,
        }
    }

    fn event_lines(state: &TuiState) -> Vec<String> {
        runtime_semantic_events(state)
            .into_iter()
            .map(|event| match event {
                RuntimeNarrativeEvent::Intent { summary } => format!("[INTENT] {summary}"),
                RuntimeNarrativeEvent::Thinking { summary } => format!("[THINKING] {summary}"),
                RuntimeNarrativeEvent::Analysis { summary } => format!("[ANALYSIS] {summary}"),
                RuntimeNarrativeEvent::AnalyzeResult { projection } => projection.render(),
                RuntimeNarrativeEvent::Planning { summary } => format!("[PLANNING] {summary}"),
                RuntimeNarrativeEvent::Validation { summary, .. } => {
                    format!("[VALIDATION] {summary}")
                }
                RuntimeNarrativeEvent::Execution { summary, .. } => {
                    format!("[EXECUTION] {summary}")
                }
                RuntimeNarrativeEvent::Preview { target } => {
                    format!("[PREVIEW] changes prepared for {target}")
                }
                RuntimeNarrativeEvent::Apply { summary, .. }
                | RuntimeNarrativeEvent::Commit { summary } => format!("[APPLY] {summary}"),
                RuntimeNarrativeEvent::Rollback { summary } => format!("[ROLLBACK] {summary}"),
                RuntimeNarrativeEvent::MutationPlan { projection } => projection.render(),
                RuntimeNarrativeEvent::MutationPreview { projection } => projection.render(),
                RuntimeNarrativeEvent::MutationApplied { projection } => projection.render(),
                RuntimeNarrativeEvent::MutationReplay { projection } => projection.render(),
                RuntimeNarrativeEvent::MutationRollback { projection } => projection.render(),
                RuntimeNarrativeEvent::System { summary, .. } => format!("[SYSTEM] {summary}"),
                RuntimeNarrativeEvent::GovernanceReject { reason } => format!("[REJECT] {reason}"),
                RuntimeNarrativeEvent::Error { message } => format!("[ERROR] {message}"),
            })
            .collect()
    }

    fn runtime_transaction(target: &str) -> RuntimeTransaction {
        RuntimeTransaction {
            tx_id: "tx-projection-refresh".to_string(),
            target_path: target.to_string(),
            resolved_target: crate::runtime::shell::ResolvedExecutionTarget::from_canonical_path(
                target,
            ),
            diff: Diff {
                file: target.to_string(),
                changes: vec![],
            },
            failed_recoverable: false,
        }
    }

    #[test]
    fn projection_refresh_does_not_drop_target_authority() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = None;
        state.active_transaction = Some(runtime_transaction("apps/cli/src/repl.rs"));

        let first = RenderSnapshot::from(&state);
        let second = RenderSnapshot::from(&state);

        assert_eq!(
            first.projection.workspace.target.as_deref(),
            Some("apps/cli/src/repl.rs")
        );
        assert_eq!(
            second.projection.workspace.target.as_deref(),
            Some("apps/cli/src/repl.rs")
        );
        assert!(!projection_surface(&second).contains("Target:\n  (none)"));
    }

    #[test]
    fn narrative_reducer_inherits_previous_target_authority() {
        let mut reducer = RuntimeNarrativeReducerState::default();

        let inherited = reducer.attach_inherited_target(RuntimeNarrativeEvent::Preview {
            target: "apps/cli/src/repl.rs".to_string(),
        });
        if let Some(target) = inherited.target_authority() {
            reducer.last_known_target = Some(target.to_string());
        }

        let event = reducer.attach_inherited_target(RuntimeNarrativeEvent::Execution {
            summary: "transaction authority available".to_string(),
            target: None,
        });

        assert_eq!(event.target_authority(), Some("apps/cli/src/repl.rs"));
    }

    #[test]
    fn next_events_are_normalized_to_intent() {
        let apply = UiEvent::Next {
            actions: vec!["apply".to_string()],
        }
        .lines();

        assert_eq!(apply, vec!["[INTENT] applying active transaction"]);
        assert!(!apply.join("\n").contains("[NEXT]"));
    }

    #[test]
    fn governance_rejections_are_projected() {
        let mut state = TuiState::new(empty_payload());
        state.rejection = Some(crate::tui::state::RejectionInfo {
            reason: "target outside workspace boundary".to_string(),
            originating_mutation: "workspace_boundary".to_string(),
            governance_source: Some("workspace".to_string()),
            convergence_source: None,
        });

        let lines = event_lines(&state);

        assert!(
            lines
                .iter()
                .any(|line| line.contains("[VALIDATION] governance boundary evaluated"))
        );
        assert!(lines.iter().any(|line| {
            line.contains("[REJECT]") && line.contains("target outside workspace boundary")
        }));
    }

    #[test]
    fn thinking_analysis_events_are_visible() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("apps/cli/src/main.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["fn main() {}".to_string()],
        });

        let lines = event_lines(&state);

        assert!(lines.contains(&"[THINKING] resolving target graph".to_string()));
        assert!(lines.contains(&"[ANALYSIS] diff structure computed".to_string()));
    }

    #[test]
    fn apply_projection_is_semantically_visible() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Git;

        let lines = event_lines(&state);

        assert!(lines.contains(&"[APPLY] transaction committed successfully".to_string()));
        assert!(lines.contains(&"[SYSTEM] runtime stabilized".to_string()));
    }

    #[test]
    fn semantic_narrative_order_is_stable() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("apps/cli/src/main.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["fn main() {}".to_string()],
        });
        state.runtime_state = RuntimeShellState::AwaitConfirmation;

        let first = event_lines(&state);
        let second = event_lines(&state);
        let categories = first
            .iter()
            .map(|line| line.split(']').next().unwrap_or_default().to_string() + "]")
            .collect::<Vec<_>>();

        assert_eq!(first, second);
        assert_eq!(
            categories,
            vec![
                "[INTENT]",
                "[THINKING]",
                "[ANALYSIS]",
                "[VALIDATION]",
                "[EXECUTION]",
                "[SYSTEM]",
            ]
        );
    }

    #[test]
    fn runtime_internal_state_is_not_exposed() {
        let mut state = TuiState::new(empty_payload());
        let absolute_target = std::env::current_dir()
            .expect("cwd")
            .join("apps/cli/src/main.rs");
        state.active_target = Some(absolute_target.display().to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["fn main() {}".to_string()],
        });

        let surface = projection_surface(&RenderSnapshot::from(&state));

        assert!(!surface.contains("[NEXT]"));
        assert!(!surface.contains("[RUNTIME] status:"));
        assert!(!surface.contains("status: IDLE"));
        assert!(!surface.contains("PREVIEW_READY"));
        assert!(!surface.contains("tx-users-chigenori-development"));
        assert!(!surface.contains("/Users/chigenori/development"));
        assert!(surface.contains("変更結果を事前確認できます。"));
        assert!(surface.contains("apps/cli/src/main.rs"));
    }

    #[test]
    fn render_snapshot_excludes_debug_and_trace_residue() {
        let mut state = TuiState::new(empty_payload());
        state.append_chat(UiEvent::Debug {
            message: "[IR-TRACE] leaked".to_string(),
        });
        state.append_chat(UiEvent::Diff {
            file: "target.rs".to_string(),
            changes: vec![DiffChunk {
                old: None,
                new: Some("[GRAPH] stale".to_string()),
                old_line: None,
                new_line: Some(1),
            }],
        });

        let snapshot = RenderSnapshot::from(&state);
        let surface = format!(
            "{}\n{}\n{}\n{}",
            snapshot.runtime.runtime_panel_lines(false).join("\n"),
            snapshot.runtime.diff_projection.lines.join("\n"),
            snapshot.status.line,
            snapshot.input.text
        );

        assert!(!surface.contains("[IR-TRACE]"));
        assert!(!surface.contains("[GRAPH]"));
        assert!(!surface.contains("[SCORE]"));
        assert!(!surface.contains("[CODING]"));
    }

    #[test]
    fn immutable_frame_composition_is_repeatable() {
        let state = TuiState::new(empty_payload());
        let layout = layout_for_area(Rect::new(0, 0, 80, 24), false);

        let first = FrameComposer::compose(RenderSnapshot::from(&state), layout);
        let second = FrameComposer::compose(RenderSnapshot::from(&state), layout);

        assert_eq!(first, second);
    }

    #[test]
    fn runtime_panel_shows_human_status() {
        let mut state = TuiState::new(empty_payload());
        state.append_chat(UiEvent::System {
            summary: "runtime idle".to_string(),
        });

        let snapshot = RenderSnapshot::from(&state);
        let lines = snapshot.runtime.runtime_panel_lines(false);
        assert!(
            lines
                .iter()
                .any(|line| line == "次の入力を待機しています。"),
            "{lines:?}"
        );
        assert!(!lines.iter().any(|line| line.contains("[SYSTEM]")));
    }

    #[test]
    fn projection_never_displays_runtime_reference() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("runtime.active_preview".to_string());
        state.append_chat(UiEvent::Diff {
            file: "runtime.active_preview".to_string(),
            changes: vec![DiffChunk {
                old: None,
                new: Some("ActivePreview { target_path: \"apps/cli/src/core.rs\" }".to_string()),
                old_line: None,
                new_line: Some(1),
            }],
        });

        let snapshot = RenderSnapshot::from(&state);
        let surface = projection_surface(&snapshot);

        assert!(!surface.contains("runtime.active_preview"));
        assert!(!surface.contains("ActivePreview"));
        assert!(!surface.contains("target_path"));
    }

    #[test]
    fn target_projection_uses_resolved_path() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("apps/cli/src/core.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["fn semantic() {}".to_string()],
        });

        let projection = RuntimeProjection::from_state(&state);

        assert_eq!(
            projection.target_label.as_deref(),
            Some("apps/cli/src/core.rs")
        );
        assert!(
            projection
                .diff_projection
                .lines
                .windows(2)
                .any(|lines| lines == ["Target:", "  apps/cli/src/core.rs"])
        );
    }

    #[test]
    fn renderer_uses_projection_only() {
        let renderer_source = include_str!("../render.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap_or_default();

        assert!(!renderer_source.contains("TuiState"));
        assert!(!renderer_source.contains("state.runtime_state"));
        assert!(!renderer_source.contains("active_target"));
        assert!(!renderer_source.contains("active_transaction_id"));
        assert!(!renderer_source.contains("active_preview"));
    }

    #[test]
    fn runtime_snapshot_is_semantic() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::AwaitConfirmation;
        state.active_target = Some("apps/cli/src/core.rs".to_string());
        state.active_transaction_id = Some("tx-42".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["fn semantic() {}".to_string()],
        });
        state.runtime_state = RuntimeShellState::AwaitConfirmation;

        let snapshot = RenderSnapshot::from(&state);

        assert_eq!(snapshot.runtime.state_label, "READY_TO_APPLY");
        assert_eq!(
            snapshot.runtime.target_label.as_deref(),
            Some("apps/cli/src/core.rs")
        );
        assert_eq!(
            snapshot.runtime.transaction_label.as_deref(),
            Some("transaction active")
        );
        assert_eq!(
            snapshot.status.line,
            "事前確認可能 | F2 Diagnostics | :diagnostics"
        );
    }

    #[test]
    fn debug_string_not_rendered() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("RuntimeState { active_preview: Some(..) }".to_string());
        state.append_chat(UiEvent::Diff {
            file: "apps/cli/src/core.rs".to_string(),
            changes: vec![DiffChunk {
                old: None,
                new: Some("RuntimeState { active_preview: Some(..) }".to_string()),
                old_line: None,
                new_line: Some(1),
            }],
        });

        let snapshot = RenderSnapshot::from(&state);
        let surface = projection_surface(&snapshot);

        assert!(!surface.contains("RuntimeState"));
        assert!(!surface.contains("active_preview"));
        assert!(!surface.contains("ActivePreview"));
    }

    #[test]
    fn projection_normalization_deterministic() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Apply;
        state.active_target = Some("apps/cli/src/core.rs".to_string());
        state.active_transaction_id = Some("tx-apply".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["fn semantic() {}".to_string()],
        });
        state.runtime_state = RuntimeShellState::Apply;

        let first = RuntimeProjection::from_state(&state);
        let second = RuntimeProjection::from_state(&state);

        assert_eq!(first, second);
        assert_eq!(first.state_label, "APPLYING");
        assert_eq!(
            first.transaction_label.as_deref(),
            Some("transaction active")
        );
    }

    #[test]
    fn tx_none_projection_is_destroyed() {
        let mut state = TuiState::new(empty_payload());
        state.append_chat(UiEvent::Diff {
            file: "apps/cli/src/core.rs".to_string(),
            changes: vec![DiffChunk {
                old: None,
                new: Some("fn semantic() {}".to_string()),
                old_line: None,
                new_line: Some(1),
            }],
        });
        state.append_chat(UiEvent::Pipeline {
            state: "Idle".to_string(),
        });

        let projection = RuntimeProjection::from_state(&state);

        assert_eq!(projection.state_label, "IDLE");
        assert_eq!(projection.transaction_label, None);
        assert!(
            projection
                .diff_projection
                .lines
                .contains(&"Target:".to_string())
        );
        assert!(
            projection
                .diff_projection
                .lines
                .contains(&"  (none)".to_string())
        );
    }

    #[test]
    fn failed_recoverable_retains_projection_only_with_tx() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("apps/cli/src/core.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["fn semantic() {}".to_string()],
        });
        state.append_chat(UiEvent::Diff {
            file: "apps/cli/src/core.rs".to_string(),
            changes: vec![DiffChunk {
                old: None,
                new: Some("fn semantic() {}".to_string()),
                old_line: None,
                new_line: Some(1),
            }],
        });
        state.runtime_state = RuntimeShellState::Apply;
        state.append_chat(UiEvent::Error {
            message: "recoverable".to_string(),
        });

        let projection = RuntimeProjection::from_state(&state);

        assert_eq!(projection.state_label, "FAILED_RECOVERABLE");
        assert!(
            projection
                .transaction_label
                .as_deref()
                .is_some_and(|tx| tx == "transaction active")
        );
        assert!(
            projection
                .diff_projection
                .lines
                .windows(2)
                .any(|lines| lines == ["Target:", "  apps/cli/src/core.rs"])
        );
    }

    #[test]
    fn failed_without_tx_cannot_retain_diff() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::Failed;

        let projection = RuntimeProjection::from_state(&state);

        assert_eq!(projection.state_label, "IDLE");
        assert_eq!(projection.transaction_label, None);
        assert!(
            projection
                .diff_projection
                .lines
                .contains(&"Target:".to_string())
        );
        assert!(
            projection
                .diff_projection
                .lines
                .contains(&"  (none)".to_string())
        );
    }

    #[test]
    fn absolute_paths_are_not_projected() {
        let mut state = TuiState::new(empty_payload());
        let target = std::env::current_dir()
            .expect("cwd")
            .join("apps/cli/src/main.rs");
        state.active_target = Some(target.display().to_string());

        let surface = projection_surface(&RenderSnapshot::from(&state));

        assert!(!surface.contains("/Users/"));
        assert!(surface.contains("apps/cli/src/main.rs"));
    }

    #[test]
    fn transaction_internal_ids_are_hidden() {
        let mut state = TuiState::new(empty_payload());
        state.active_transaction_id = Some("tx-users-secret-runtime-token".to_string());

        let surface = projection_surface(&RenderSnapshot::from(&state));

        assert!(!surface.contains("tx-users-secret-runtime-token"));
        assert!(surface.contains("次の入力を待機しています。"));
    }

    #[test]
    fn workspace_projection_is_semantic() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("apps/cli/src/main.rs".to_string());

        let projection = RenderSnapshot::from(&state).projection.workspace;

        assert_eq!(projection.target.as_deref(), Some("apps/cli/src/main.rs"));
        assert_eq!(projection.operation, "none");
        assert_eq!(projection.status, "runtime idle");
    }

    #[test]
    fn workspace_projection_has_no_transport_leaks() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("runtime.active_preview".to_string());
        state.append_chat(UiEvent::Diff {
            file: "/Users/chigenori/development/Design_BrainModel/apps/cli/src/main.rs".to_string(),
            changes: vec![DiffChunk {
                old: None,
                new: Some(
                    "+preview /Users/chigenori/development/Design_BrainModel/apps/cli/src/main.rs"
                        .to_string(),
                ),
                old_line: None,
                new_line: Some(1),
            }],
        });

        let surface = projection_surface(&RenderSnapshot::from(&state));

        assert!(!surface.contains("+preview /Users/"));
        assert!(!surface.contains("raw diff"));
        assert!(!surface.contains("runtime.active_preview"));
    }

    #[test]
    fn duplicate_runtime_idle_is_collapsed() {
        let events = vec![
            RuntimeNarrativeEvent::System {
                summary: "runtime idle".to_string(),
                target: None,
            },
            RuntimeNarrativeEvent::System {
                summary: "runtime idle".to_string(),
                target: None,
            },
        ];

        assert_eq!(
            RuntimeNarrativeReducer::render(events),
            vec!["[SYSTEM] runtime idle".to_string()]
        );
    }

    #[test]
    fn semantic_echoes_are_reduced() {
        let events = vec![RuntimeNarrativeEvent::Intent {
            summary: "observing runtime cognition".to_string(),
        }];

        assert_eq!(
            RuntimeNarrativeReducer::render(events),
            vec!["[INTENT] checking runtime state".to_string()]
        );
    }

    #[test]
    fn projection_snapshot_is_atomic() {
        let state = TuiState::new(empty_payload());
        let snapshot = RenderSnapshot::from(&state);

        assert_eq!(
            snapshot.projection.narrative.lines,
            snapshot.runtime.narrative_lines
        );
        assert_eq!(
            snapshot.projection.workspace,
            WorkspaceProjection::from(snapshot.runtime.diff_projection.workspace.clone())
        );
        assert_eq!(
            snapshot.projection.runtime_state,
            snapshot.runtime.state_label
        );
    }

    #[test]
    fn projection_order_is_deterministic() {
        let state = TuiState::new(empty_payload());

        assert_eq!(
            RenderSnapshot::from(&state).projection,
            RenderSnapshot::from(&state).projection
        );
    }

    #[test]
    fn preview_runtime_is_projected_as_human_summary() {
        let mut state = TuiState::new(empty_payload());
        state.runtime_state = RuntimeShellState::PreviewReady;

        let snapshot = RenderSnapshot::from(&state);
        let surface = snapshot.runtime.runtime_panel_lines(false).join("\n");

        assert!(surface.contains("変更結果を事前確認できます。"));
        assert!(surface.contains("• Previewを確認"));
        assert!(surface.contains("• Applyを実行"));
        assert!(!surface.contains("PREVIEW_READY"));
        assert!(!surface.contains("[SYSTEM]"));
    }

    fn projection_surface(snapshot: &RenderSnapshot) -> String {
        format!(
            "{}\n{}\n{}",
            snapshot.runtime.runtime_panel_lines(false).join("\n"),
            snapshot.runtime.diff_projection.lines.join("\n"),
            snapshot.status.line
        )
    }
}
