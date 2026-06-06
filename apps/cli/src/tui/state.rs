use std::collections::VecDeque;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use strategy_engine::ExecutionPlanCandidate;

pub use crate::core::{
    Constraint, CoreState, DesignDocument, Diff, DiffChunk, ReasonUnit, StructureTree,
};
use crate::nl::language::detect_runtime_language;
use crate::nl::normalization::normalize_runtime_input;
use crate::nl::planner::InstructionPlan;
use crate::nl::types::SupportedLanguage;
use crate::pipeline::PipelineState;
use crate::runtime::autonomous::{ExecutionMemory, ExecutionSession};
use crate::runtime::branch::BranchRuntime;
use crate::runtime::coordination::{
    CoordinationMemory, RuntimeNode, RuntimeRole, SharedWorldState,
};
use crate::runtime::governance::{CognitivePolicy, GovernanceMemory, GovernanceState};
use crate::runtime::runtime_events::DebugEvent;
use crate::runtime::synthesis::ArchitectureMemory;
use crate::tui::design_convergence::DesignConvergenceState;
use crate::tui::input::{PersistentInputHistory, complete_command};
use crate::tui::runtime::RuntimeShellState;
use crate::tui::workspace::{WorkspaceProjector, WorkspaceState};

use super::model::UiPayload;

pub const MAX_CHAT_LINES: usize = 1000;
pub const MAX_EVENTS: usize = 2000;
pub const DESIGN_MAX_LINES: usize = 20;
pub const MAX_SPEC_LINES: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Input,
    Chat,
    Design,
}

impl Focus {
    pub fn next(self) -> Self {
        match self {
            Self::Input => Self::Chat,
            Self::Chat => Self::Design,
            Self::Design => Self::Input,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Input => Self::Design,
            Self::Chat => Self::Input,
            Self::Design => Self::Chat,
        }
    }
}

pub const BANNED_SURFACE_TOKENS: &[&str] = &[
    "[IR-TRACE]",
    "[GRAPH]",
    "[SCORE]",
    "[CODING]",
    "TRACE:",
    "[ROUTE]",
    "[EXECUTE]",
    "[ANALYZE]",
    "[SPEC_CONTEXT]",
    "[DOMAIN]",
    "[STRUCTURAL_DIAGNOSIS]",
    "[CORE_DIAGNOSIS]",
    "[RUNTIME_DIAGNOSIS]",
    "[UI_DIAGNOSIS]",
    "[REPAIR_PLANNING]",
    "[UI_REPAIR_PLAN]",
    "[IMPLEMENTATION_PLANNING]",
    "[UI_IMPLEMENTATION_PLAN]",
    "[REPAIR_PLAN]",
    "[IMPLEMENTATION_PLAN]",
    "[KEY_TRACE]",
    "[SUBMIT_TRACE]",
    "[RUNTIME_ROUTE_TRACE]",
    "[CORE_SUBMIT_TRACE]",
    "[WORKSPACE_TRACE]",
    "[SNAPSHOT_TRACE]",
    "[RENDER_TRACE]",
    "[TRACE]",
    "runtime.active_preview",
    "RuntimeState",
    "ActivePreview",
    "PreviewDiff",
    "Rect {",
    "tx-users-",
    "active_preview",
];

const RUNTIME_REFERENCE_TOKENS: &[&str] = &[
    "runtime.active_preview",
    "RuntimeState",
    "ActivePreview",
    "PreviewDiff",
    "Rect {",
    "tx-users-",
    "active_preview",
];

pub fn sanitize_line(line: &str) -> Option<String> {
    if BANNED_SURFACE_TOKENS
        .iter()
        .any(|token| line.contains(token))
    {
        None
    } else {
        Some(line.to_string())
    }
}

pub fn contains_runtime_reference(line: &str) -> bool {
    RUNTIME_REFERENCE_TOKENS
        .iter()
        .any(|token| line.contains(token))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeNarrativeEvent {
    Intent {
        summary: String,
    },
    Thinking {
        summary: String,
    },
    Analysis {
        summary: String,
    },
    AnalyzeResult {
        projection: AnalyzeProjection,
    },
    Planning {
        summary: String,
    },
    Validation {
        summary: String,
        target: Option<String>,
    },
    Execution {
        summary: String,
        target: Option<String>,
    },
    Preview {
        target: String,
    },
    Apply {
        summary: String,
        target: Option<String>,
    },
    Commit {
        summary: String,
    },
    Rollback {
        summary: String,
    },
    MutationPlan {
        projection: MutationProjection,
    },
    MutationPreview {
        projection: PreviewProjection,
    },
    MutationApplied {
        projection: MutationProjection,
    },
    MutationReplay {
        projection: ReplayProjection,
    },
    MutationRollback {
        projection: RollbackProjection,
    },
    System {
        summary: String,
        target: Option<String>,
    },
    GovernanceReject {
        reason: String,
    },
    Error {
        message: String,
    },
}

impl RuntimeNarrativeEvent {
    pub fn render(&self) -> String {
        match self {
            Self::Intent { summary }
            | Self::Thinking { summary }
            | Self::Analysis { summary }
            | Self::Planning { summary }
            | Self::Validation { summary, .. }
            | Self::Execution { summary, .. }
            | Self::Apply { summary, .. }
            | Self::Commit { summary }
            | Self::Rollback { summary }
            | Self::System { summary, .. } => summary.clone(),
            Self::Preview { target } => format!("changes prepared for {target}"),
            Self::MutationPlan { projection } => projection.render(),
            Self::MutationPreview { projection } => projection.render(),
            Self::MutationApplied { projection } => projection.render(),
            Self::MutationReplay { projection } => projection.render(),
            Self::MutationRollback { projection } => projection.render(),
            Self::GovernanceReject { reason } => format!("rejected: {}", reason),
            Self::Error { message } => format!("[ERROR] {}", message),
            Self::AnalyzeResult { projection } => projection.render(),
        }
    }

    pub fn target_authority(&self) -> Option<&str> {
        match self {
            Self::Preview { target } => Some(target.as_str()),
            Self::Validation { target, .. }
            | Self::Execution { target, .. }
            | Self::Apply { target, .. }
            | Self::System { target, .. } => target.as_deref(),
            Self::AnalyzeResult { projection } => Some(projection.target.as_str()),
            Self::MutationPlan { projection } => Some(projection.target.as_str()),
            Self::MutationPreview { projection } => Some(projection.mutation_id.as_str()),
            Self::MutationApplied { projection } => Some(projection.target.as_str()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzeProjection {
    pub target: String,
    pub project_name: String,
    pub module_count: usize,
    pub dependency_cycles: usize,
    pub coupling_level: String,
    pub findings: Vec<String>,
    pub mutation_candidates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationProjection {
    pub mutation_id: String,
    pub target: String,
    pub operation: String,
    pub validation_targets: Vec<String>,
    pub expected_improvements: Vec<String>,
}

impl MutationProjection {
    pub fn render(&self) -> String {
        let mut lines = vec![
            "Mutation Plan".to_string(),
            "-------------".to_string(),
            format!("ID: {}", self.mutation_id),
            format!("Target: {}", self.target),
            format!("Operation: {}", self.operation),
            "Validation Targets".to_string(),
        ];
        lines.extend(
            self.validation_targets
                .iter()
                .map(|target| format!("* {target}")),
        );
        lines.push("Expected Improvements".to_string());
        lines.extend(
            self.expected_improvements
                .iter()
                .map(|improvement| format!("* {improvement}")),
        );
        lines.join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewProjection {
    pub mutation_id: String,
    pub affected_modules: Vec<String>,
    pub affected_files: Vec<String>,
    pub structural_impact: String,
}

impl PreviewProjection {
    pub fn render(&self) -> String {
        let mut lines = vec![
            "Mutation Preview".to_string(),
            "----------------".to_string(),
            format!("ID: {}", self.mutation_id),
            "Affected Modules:".to_string(),
        ];
        lines.extend(
            self.affected_modules
                .iter()
                .map(|module| format!("- {module}")),
        );
        lines.push("Affected Files:".to_string());
        lines.extend(self.affected_files.iter().map(|file| format!("- {file}")));
        lines.push(format!("Structural Impact: {}", self.structural_impact));
        lines.join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayProjection {
    pub mutation_id: String,
    pub status: String,
    pub checksum_matched: bool,
}

impl ReplayProjection {
    pub fn render(&self) -> String {
        format!(
            "Mutation Replay: {} - {} (Checksum Matched: {})",
            self.mutation_id, self.status, self.checksum_matched
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackProjection {
    pub mutation_id: String,
    pub status: String,
}

impl RollbackProjection {
    pub fn render(&self) -> String {
        format!("Mutation Rollback: {} - {}", self.mutation_id, self.status)
    }
}

impl AnalyzeProjection {
    pub fn render(&self) -> String {
        let mut lines = vec![
            "Analyze Result".to_string(),
            "--------------".to_string(),
            format!("Project: {}", self.project_name),
            format!("Modules: {}", self.module_count),
            format!("Dependency Cycles: {}", self.dependency_cycles),
            format!("Coupling: {}", self.coupling_level),
            "Findings".to_string(),
        ];
        lines.extend(self.findings.iter().map(|finding| format!("- {finding}")));
        lines.push("Mutation Candidates".to_string());
        lines.extend(
            self.mutation_candidates
                .iter()
                .enumerate()
                .map(|(index, candidate)| format!("{}. {candidate}", index + 1)),
        );
        lines.join("\n")
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    Intent {
        summary: String,
    },
    Thinking {
        summary: String,
    },
    Analysis {
        summary: String,
    },
    AnalyzeResult {
        projection: AnalyzeProjection,
    },
    Planning {
        summary: String,
    },
    Validation {
        summary: String,
    },
    Editing {
        target: String,
        action: String,
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
    Pipeline {
        state: String,
    },
    MutationPlan {
        projection: MutationProjection,
    },
    MutationPreview {
        projection: PreviewProjection,
    },
    MutationApplied {
        projection: MutationProjection,
    },
    MutationReplay {
        projection: ReplayProjection,
    },
    MutationRollback {
        projection: RollbackProjection,
    },
    SpecContext {
        context: crate::specification_bridge::SpecificationContext,
    },
    DomainClassification {
        domain: String,
    },
    StructuralDiagnosis {
        result: crate::specification_bridge::StructuralDiagnosisResult,
    },
    RepairPlan {
        plan: crate::specification_bridge::RepairPlan,
    },
    ImplementationPlan {
        plan: crate::specification_bridge::ImplementationPlan,
    },
    Runtime {
        message: String,
    },
    Apply {
        summary: String,
    },
    Rollback {
        summary: String,
    },
    System {
        summary: String,
    },
    Reject {
        reason: String,
    },
    Next {
        actions: Vec<String>,
    },
    Error {
        message: String,
    },
    ErrorRecovery {
        candidates: Vec<ExecutionPlanCandidate>,
    },
    Debug {
        message: String,
    },
    /// Structured execution proposal.  Spec DBM-EXECUTION-CANDIDATE-SPEC §9.
    Proposal {
        candidates: Vec<ExecutionPlanCandidate>,
    },
}

impl UiEvent {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Intent { .. } => "INTENT",
            Self::Thinking { .. } => "THINKING",
            Self::Analysis { .. } => "ANALYSIS",
            Self::AnalyzeResult { .. } => "ANALYZE RESULT",
            Self::Planning { .. } => "PLANNING",
            Self::Validation { .. } => "VALIDATION",
            Self::Editing { .. } => "EDITING",
            Self::Plan { .. } => "PLAN",
            Self::Execution { .. } => "EXECUTION",
            Self::Preview { .. } => "PREVIEW",
            Self::Diff { .. } => "DIFF",
            Self::Result { .. } => "RESULT",
            Self::DesignUpdate { .. } => "DESIGN",
            Self::DesignDiff { .. } => "DESIGN DIFF",
            Self::Pipeline { .. } => "PIPELINE",
            Self::MutationPlan { .. } => "MUTATION PLAN",
            Self::MutationPreview { .. } => "MUTATION PREVIEW",
            Self::MutationApplied { .. } => "MUTATION APPLIED",
            Self::MutationReplay { .. } => "MUTATION REPLAY",
            Self::MutationRollback { .. } => "MUTATION ROLLBACK",
            Self::SpecContext { .. } => "SPEC_CONTEXT",
            Self::DomainClassification { .. } => "DOMAIN",
            Self::StructuralDiagnosis { .. } => "STRUCTURAL_DIAGNOSIS",
            Self::RepairPlan { .. } => "REPAIR_PLAN",
            Self::ImplementationPlan { .. } => "IMPLEMENTATION_PLAN",
            Self::Runtime { .. } => "RUNTIME",
            Self::Apply { .. } => "APPLY",
            Self::Rollback { .. } => "ROLLBACK",
            Self::System { .. } => "SYSTEM",
            Self::Reject { .. } => "REJECT",
            Self::Next { .. } => "INTENT",
            Self::Error { .. } => "ERROR",
            Self::ErrorRecovery { .. } => "RECOVERY",
            Self::Debug { .. } => "DEBUG",
            Self::Proposal { .. } => "PROPOSAL",
        }
    }

    pub fn text(&self) -> String {
        match self {
            Self::Intent { summary } => summary.clone(),
            Self::Thinking { summary } => summary.clone(),
            Self::Analysis { summary } => summary.clone(),
            Self::AnalyzeResult { projection } => projection.render(),
            Self::Planning { summary } => summary.clone(),
            Self::Validation { summary } => summary.clone(),
            Self::Editing { target, action } => format!("{target}: {action}"),
            Self::Plan { steps } => steps.join("\n"),
            Self::Execution { step } => step.clone(),
            Self::Preview { diff } => format!("Preview generated ({} lines)", diff.len()),
            Self::Diff { file, changes } => {
                format!("Previewed {} changes to {}", changes.len(), file)
            }
            Self::Result { message }
            | Self::Runtime { message }
            | Self::Error { message }
            | Self::Debug { message } => message.clone(),
            Self::DesignUpdate { summary, score } => format!("Score: {score:.2}\n- {summary}"),
            Self::DesignDiff { changes } => changes.join("\n"),
            Self::Pipeline { state } => state.clone(),
            Self::MutationPlan { projection } => projection.render(),
            Self::MutationPreview { projection } => projection.render(),
            Self::MutationApplied { projection } => projection.render(),
            Self::MutationReplay { projection } => projection.render(),
            Self::MutationRollback { projection } => projection.render(),
            Self::SpecContext { context } => format!(
                "system_name={}\ngoals={}\nconstraints={}\ncomponents={}\nrules={}",
                context.system_name.as_deref().unwrap_or("(none)"),
                context.goals.len(),
                context.constraints.len(),
                context.architecture.len(),
                context.rules.len()
            ),
            Self::DomainClassification { domain } => domain.clone(),
            Self::StructuralDiagnosis { result } => format!(
                "violations={} warnings={}",
                result.violations.len(),
                result.warnings.len()
            ),
            Self::RepairPlan { plan } => format!(
                "suggestions={} steps={}",
                plan.suggestions.len(),
                plan.execution_steps.len()
            ),
            Self::ImplementationPlan { plan } => format!(
                "tasks={} validations={}",
                plan.tasks.len(),
                plan.validations.len()
            ),
            Self::Apply { summary } => summary.clone(),
            Self::Rollback { summary } => summary.clone(),
            Self::System { summary } => summary.clone(),
            Self::Reject { reason } => reason.clone(),
            Self::Next { actions } => actions
                .iter()
                .map(|action| normalize_intent_summary(action))
                .collect::<Vec<_>>()
                .join("\n"),
            Self::ErrorRecovery { candidates } => {
                let mut lines = vec!["Retry candidates:".to_string()];
                for candidate in candidates {
                    lines.extend(candidate.render_lines());
                    lines.push(String::new());
                }
                lines.join("\n")
            }
            Self::Proposal { candidates } => {
                // Render top candidates per spec §9 表示例
                let mut lines: Vec<String> = Vec::new();
                for c in candidates {
                    lines.extend(c.render_lines());
                    lines.push(String::new());
                }
                lines.join("\n")
            }
        }
    }

    pub fn lines(&self) -> Vec<String> {
        let prefix = format!("[{}] ", self.label());
        let text = self.text();
        if text.is_empty() {
            return vec![prefix.trim_end().to_string()];
        }
        text.lines()
            .enumerate()
            .map(|(idx, line)| {
                if idx == 0 {
                    format!("{prefix}{line}")
                } else {
                    format!("{}{}", " ".repeat(prefix.len()), line)
                }
            })
            .collect()
    }
}

fn normalize_intent_summary(action: &str) -> String {
    match action.trim().to_ascii_lowercase().as_str() {
        "apply" => "applying active transaction".to_string(),
        "status" => "checking runtime state".to_string(),
        "rollback" => "reverting active transaction".to_string(),
        other if other.starts_with("preview") => "preparing transaction preview".to_string(),
        _ => action.to_string(),
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EventQueue {
    queue: VecDeque<UiEvent>,
}

impl EventQueue {
    pub fn push(&mut self, event: UiEvent) {
        self.queue.push_back(event);
        while self.queue.len() > MAX_EVENTS {
            self.queue.pop_front();
        }
    }

    pub fn pop(&mut self) -> Option<UiEvent> {
        self.queue.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatScrollState {
    pub is_following: bool,
    pub offset: usize,
}

impl ChatScrollState {
    pub fn user_scroll_up(&mut self, amount: usize) {
        self.is_following = false;
        self.offset = self.offset.saturating_add(amount);
    }

    pub fn user_scroll_down(&mut self, amount: usize) {
        self.offset = self.offset.saturating_sub(amount);
        if self.offset == 0 {
            self.is_following = true;
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        self.offset = 0;
        self.is_following = true;
    }

    pub fn apply_append(&mut self) {
        if self.is_following {
            self.scroll_to_bottom();
        }
    }
}

impl Default for ChatScrollState {
    fn default() -> Self {
        Self {
            is_following: true,
            offset: 0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InputBuffer {
    pub text: String,
    pub cursor: usize,
}

impl InputBuffer {
    pub fn insert_char(&mut self, ch: char) {
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub fn insert_newline(&mut self) {
        if self.line_count() < 3 {
            self.insert_char('\n');
        }
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        if let Some((idx, _)) = self.text[..self.cursor].char_indices().next_back() {
            self.text.replace_range(idx..self.cursor, "");
            self.cursor = idx;
        }
    }

    pub fn delete(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let next = self.text[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(offset, _)| self.cursor + offset)
            .unwrap_or(self.text.len());
        self.text.replace_range(self.cursor..next, "");
    }

    pub fn move_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        if let Some((idx, _)) = self.text[..self.cursor].char_indices().next_back() {
            self.cursor = idx;
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        self.cursor = self.text[self.cursor..]
            .char_indices()
            .nth(1)
            .map(|(offset, _)| self.cursor + offset)
            .unwrap_or(self.text.len());
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn set_text(&mut self, text: String) {
        self.cursor = text.len();
        self.text = text;
    }

    pub fn line_count(&self) -> usize {
        self.text.lines().count().max(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecificationEditor {
    pub lines: Vec<String>,
    pub cursor_row: usize,
    pub cursor_col: usize,
}

impl Default for SpecificationEditor {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
        }
    }
}

impl SpecificationEditor {
    pub fn insert_char(&mut self, ch: char) {
        self.ensure_editable_line();
        let row = self.cursor_row;
        let col = self.cursor_col.min(self.lines[row].len());
        self.lines[row].insert(col, ch);
        self.cursor_col = col + ch.len_utf8();
    }

    pub fn insert_newline(&mut self) {
        if self.lines.len() >= MAX_SPEC_LINES {
            return;
        }
        self.ensure_editable_line();
        let row = self.cursor_row;
        let col = self.cursor_col.min(self.lines[row].len());
        let remainder = self.lines[row].split_off(col);
        self.lines.insert(row + 1, remainder);
        self.cursor_row = row + 1;
        self.cursor_col = 0;
    }

    pub fn backspace(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        self.clamp_cursor();
        if self.cursor_col == 0 {
            if self.cursor_row == 0 {
                return;
            }
            // YAML構造保護: 前の行がYAMLキー行（末尾 ':'）の場合、
            // 行結合を禁止してYAMLドキュメント構造を保持する。
            let prev_line = &self.lines[self.cursor_row - 1];
            if prev_line.trim_end().ends_with(':') {
                return;
            }
            let current = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].len();
            self.lines[self.cursor_row].push_str(&current);
            return;
        }
        let line = &mut self.lines[self.cursor_row];
        if let Some((idx, _)) = line[..self.cursor_col].char_indices().next_back() {
            line.replace_range(idx..self.cursor_col, "");
            self.cursor_col = idx;
        }
    }

    pub fn delete(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        self.clamp_cursor();
        let row = self.cursor_row;
        if self.cursor_col >= self.lines[row].len() {
            if row + 1 < self.lines.len() {
                let next = self.lines.remove(row + 1);
                self.lines[row].push_str(&next);
            }
            return;
        }
        let next = self.lines[row][self.cursor_col..]
            .char_indices()
            .nth(1)
            .map(|(offset, _)| self.cursor_col + offset)
            .unwrap_or(self.lines[row].len());
        self.lines[row].replace_range(self.cursor_col..next, "");
    }

    pub fn move_left(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        self.clamp_cursor();
        if self.cursor_col == 0 {
            if self.cursor_row > 0 {
                self.cursor_row -= 1;
                self.cursor_col = self.lines[self.cursor_row].len();
            }
            return;
        }
        if let Some((idx, _)) = self.lines[self.cursor_row][..self.cursor_col]
            .char_indices()
            .next_back()
        {
            self.cursor_col = idx;
        }
    }

    pub fn move_right(&mut self) {
        if self.lines.is_empty() {
            return;
        }
        self.clamp_cursor();
        let line_len = self.lines[self.cursor_row].len();
        if self.cursor_col >= line_len {
            if self.cursor_row + 1 < self.lines.len() {
                self.cursor_row += 1;
                self.cursor_col = 0;
            }
            return;
        }
        self.cursor_col = self.lines[self.cursor_row][self.cursor_col..]
            .char_indices()
            .nth(1)
            .map(|(offset, _)| self.cursor_col + offset)
            .unwrap_or(line_len);
    }

    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.clamp_cursor();
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.clamp_cursor();
        }
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.cursor_row = 0;
        self.cursor_col = 0;
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn line_count(&self) -> usize {
        self.lines.len().max(1)
    }

    pub fn char_count(&self) -> usize {
        self.text().chars().count()
    }

    fn ensure_editable_line(&mut self) {
        if self.lines.is_empty() {
            self.lines.push(String::new());
            self.cursor_row = 0;
            self.cursor_col = 0;
        }
        self.clamp_cursor();
    }

    fn clamp_cursor(&mut self) {
        if self.lines.is_empty() {
            self.cursor_row = 0;
            self.cursor_col = 0;
            return;
        }
        self.cursor_row = self.cursor_row.min(self.lines.len() - 1);
        self.cursor_col = self.cursor_col.min(self.lines[self.cursor_row].len());
    }
}

/// YAML構造を保証するための防御的正規化。
fn ensure_yaml_line_breaks(input: &str) -> String {
    let yaml_keys = [
        "system_name:",
        "goals:",
        "constraints:",
        "architecture:",
        "rules:",
    ];
    let mut result = input.to_string();
    for key in yaml_keys.iter().rev() {
        let mut search_from = 0;
        while let Some(relative_pos) = result[search_from..].find(key) {
            let pos = search_from + relative_pos;
            if pos > 0 && result.as_bytes().get(pos - 1).copied() != Some(b'\n') {
                let line_start = result[..pos].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
                let before_on_line = result[line_start..pos].trim_start();
                if yaml_keys
                    .iter()
                    .any(|existing| before_on_line.starts_with(existing))
                {
                    result.insert(pos, '\n');
                    search_from = pos + key.len() + 1;
                    continue;
                }
            }
            search_from = pos + key.len();
        }
    }

    let list_sections = ["goals:", "constraints:", "architecture:", "rules:"];
    let mut normalized = Vec::new();
    let mut previous_was_list_section = false;
    for raw_line in result.lines() {
        let mut line = normalize_yaml_line(raw_line, &yaml_keys);
        let trimmed = line.trim_start();
        if list_sections.iter().any(|section| trimmed == *section) {
            previous_was_list_section = true;
            normalized.push(line);
            continue;
        }
        if previous_was_list_section && trimmed.starts_with("- ") {
            line = format!("  {trimmed}");
        }
        previous_was_list_section = false;
        normalized.push(line);
    }

    if input.ends_with('\n') && !normalized.is_empty() {
        format!("{}\n", normalized.join("\n"))
    } else {
        normalized.join("\n")
    }
}

fn normalize_yaml_line(raw_line: &str, yaml_keys: &[&str]) -> String {
    let indent_len = raw_line.len() - raw_line.trim_start().len();
    let indent = &raw_line[..indent_len];
    let trimmed = raw_line.trim_start();

    if let Some(rest) = trimmed.strip_prefix('-') {
        if !rest.starts_with(' ') && !rest.is_empty() {
            return format!("{indent}- {}", rest.trim_start());
        }
    }
    if let Some(rest) = trimmed.strip_prefix('*') {
        if !rest.is_empty() {
            return format!("{indent}- {}", rest.trim_start());
        }
    }

    for key in yaml_keys {
        if let Some(value) = trimmed.strip_prefix(key) {
            if !value.is_empty() {
                let before = value.as_bytes().first().copied();
                if before != Some(b'\n') {
                    return format!("{indent}{key} {}", value.trim_start());
                }
            }
        }
    }
    raw_line.to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorState {
    pub editor: SpecificationEditor,
    pub editing: bool,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            editor: SpecificationEditor::default(),
            editing: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChatState {
    pub events: Vec<UiEvent>,
}

/// UI-only session state.  Phase 4.5: all pipeline/design/proposal state has
/// moved to `CoreState`; only pure-UI fields remain here.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SessionState {
    /// Active chat-filter token (e.g. `"DIFF"` shows only `[DIFF]` lines).
    pub filter: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTransaction {
    pub tx_id: String,
    pub target_path: String,
    pub resolved_target: crate::runtime::shell::ResolvedExecutionTarget,
    pub diff: Diff,
    pub failed_recoverable: bool,
}

impl ChatState {
    pub fn append_chat(&mut self, event: UiEvent) {
        self.events.push(event);
        while self.events.len() > MAX_CHAT_LINES {
            self.events.remove(0);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectionInfo {
    pub reason: String,
    pub originating_mutation: String,
    pub governance_source: Option<String>,
    pub convergence_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TuiDiagnostics {
    pub last_event: Option<String>,
    pub last_key_event: Option<String>,
    pub last_focus_transition: Option<String>,
    pub last_mutation: Option<String>,
    pub raw_mode_active: bool,
    pub runtime_state: Option<String>,
    pub active_task: Option<String>,
    pub proposal_count: usize,
    pub followup_status: Option<String>,
    pub previous_context_used: bool,
    pub memory_status: Option<String>,
    pub replay_status: Option<String>,
    pub canonical_reuse_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyGuardSource {
    NormalPreview,
    PromotedPlan,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ApplyGuardState {
    pub transaction_id: Option<String>,
    pub target: Option<PathBuf>,
    pub source: Option<ApplyGuardSource>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TuiState {
    pub diagnostic_mode: bool,
    pub diagnostics: TuiDiagnostics,
    pub chat: ChatState,
    pub design_doc: DesignDocument,
    pub input: InputBuffer,
    pub editor_state: EditorState,
    pub focus: Focus,
    pub chat_scroll: ChatScrollState,
    pub event_queue: EventQueue,
    pub workspace: WorkspaceState,
    pub convergence: DesignConvergenceState,
    pub pipeline_state: PipelineState,
    pub session: SessionState,
    pub design_scroll: usize,
    pub design_collapsed: bool,
    pub design_updated: bool,
    pub narrative_expanded: bool,
    pub history: Vec<String>,
    history_cursor: Option<usize>,
    pub persistent_history: Option<PersistentInputHistory>,
    pub runtime_state: RuntimeShellState,
    pub active_target: Option<String>,
    pub active_transaction_id: Option<String>,
    pub active_transaction: Option<RuntimeTransaction>,
    pub apply_guard: Option<ApplyGuardState>,
    pub promoted_plan: Option<InstructionPlan>,
    pub last_applied_plan: Option<InstructionPlan>,
    pub rejection: Option<RejectionInfo>,
    pub dirty_tree_state: String,
    pub language_mode: SupportedLanguage,
    pub debug_events: Vec<DebugEvent>,
    /// Read-only cache of the last `CoreState` returned by Core.  Phase 4.5.
    /// This is the Single Source of Truth snapshot; the UI never mutates it.
    pub core_snapshot: CoreState,
    pub state_generation_id: u64,
    pub last_command_trace: Option<crate::runtime::shell::RuntimeCommandTrace>,
    pub next_command_id: u64,
    /// Branch isolation tracking.  `None` until the first successful preview
    /// commit.  Managed exclusively by `runtime::shell`.
    pub branch_runtime: Option<BranchRuntime>,
    /// Autonomous execution session.
    pub autonomous_session: Option<ExecutionSession>,
    /// Persistent memory for autonomous repairs.
    pub autonomous_memory: ExecutionMemory,
    /// Persistent memory for architecture synthesis.
    pub architecture_memory: ArchitectureMemory,
    /// Multi-runtime coordination node.
    pub runtime_node: RuntimeNode,
    /// Shared world state authority.
    pub shared_world_state: SharedWorldState,
    /// Persistent memory for distributed coordination.
    pub coordination_memory: CoordinationMemory,
    /// Meta-cognitive governance policy and lifecycle.
    pub cognitive_policy: CognitivePolicy,
    pub governance_state: GovernanceState,
    pub governance_memory: GovernanceMemory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiAction {
    None,
    Quit,
    Submit(String),
    SaveDesign,
}

impl TuiState {
    pub fn new(payload: UiPayload) -> Self {
        let design_doc = seed_design_document(&payload);
        Self {
            diagnostic_mode: false,
            diagnostics: TuiDiagnostics::default(),
            chat: ChatState {
                events: seed_chat_stream(&payload),
            },
            design_doc,
            input: InputBuffer::default(),
            editor_state: EditorState::default(),
            focus: Focus::Input,
            chat_scroll: ChatScrollState::default(),
            event_queue: EventQueue::default(),
            workspace: WorkspaceState::default(),
            convergence: DesignConvergenceState::default(),
            pipeline_state: PipelineState::default(),
            session: SessionState::default(),
            design_scroll: 0,
            design_collapsed: false,
            design_updated: false,
            narrative_expanded: false,
            history: Vec::new(),
            history_cursor: None,
            persistent_history: None,
            runtime_state: RuntimeShellState::Idle,
            active_target: None,
            active_transaction_id: None,
            active_transaction: None,
            apply_guard: None,
            promoted_plan: None,
            last_applied_plan: None,
            rejection: None,
            dirty_tree_state: "clean".to_string(),
            language_mode: SupportedLanguage::Unknown,
            debug_events: Vec::new(),
            core_snapshot: CoreState::default(),
            state_generation_id: 1,
            last_command_trace: None,
            next_command_id: 1,
            branch_runtime: None,
            autonomous_session: None,
            autonomous_memory: ExecutionMemory::default(),
            architecture_memory: ArchitectureMemory::default(),
            runtime_node: RuntimeNode::new("local-node".to_string(), RuntimeRole::Planner),
            shared_world_state: SharedWorldState::default(),
            coordination_memory: CoordinationMemory::default(),
            cognitive_policy: CognitivePolicy::default(),
            governance_state: GovernanceState::Stable,
            governance_memory: GovernanceMemory::default(),
        }
        .with_pseudo_stream()
    }

    pub fn increment_state_generation(&mut self) {
        self.state_generation_id = self.state_generation_id.saturating_add(1);
    }

    pub fn enable_persistent_history(&mut self, path: std::path::PathBuf) {
        let store = PersistentInputHistory::new(path);
        self.history = store.load();
        self.persistent_history = Some(store);
    }

    pub fn status_line(&self) -> String {
        format!(
            "state={} tx={} dirty={} target={} lang={}",
            self.runtime_state.label(),
            self.active_transaction
                .as_ref()
                .map(|tx| tx.tx_id.as_str())
                .unwrap_or("(none)"),
            self.dirty_tree_state,
            self.active_transaction
                .as_ref()
                .map(|tx| tx.target_path.as_str())
                .or(self.active_target.as_deref())
                .unwrap_or("(none)"),
            crate::nl::language::language_label(self.language_mode)
        )
    }

    pub fn sync_projection_authority(&mut self) {
        if let Some(tx) = self.active_transaction.as_ref()
            && !tx.target_path.trim().is_empty()
            && tx.target_path != "preview"
        {
            self.active_target = Some(tx.target_path.clone());
        }
        debug_assert!(
            self.active_transaction.is_none() || self.active_target.is_some(),
            "active transaction requires active target authority"
        );
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> TuiAction {
        if key.kind != KeyEventKind::Press {
            return TuiAction::None;
        }
        self.increment_state_generation();
        match key.code {
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::SUPER) => {
                return TuiAction::Quit;
            }
            KeyCode::Esc => {
                if self.focus == Focus::Input {
                    return TuiAction::None;
                }
                return TuiAction::Quit;
            }
            KeyCode::Tab => {
                let prev_focus = self.focus;
                if self.focus == Focus::Input
                    && !self.editor_state.editing
                    && let Some(completed) = complete_command(&self.input.text)
                {
                    self.input.set_text(completed);
                    return TuiAction::None;
                }
                self.focus = self.focus.next();
                if self.diagnostic_mode {
                    self.diagnostics.last_focus_transition =
                        Some(format!("{:?} -> {:?}", prev_focus, self.focus));
                }
                return TuiAction::None;
            }
            KeyCode::BackTab => {
                let prev_focus = self.focus;
                self.focus = self.focus.previous();
                if self.diagnostic_mode {
                    self.diagnostics.last_focus_transition =
                        Some(format!("{:?} -> {:?}", prev_focus, self.focus));
                }
                return TuiAction::None;
            }
            KeyCode::F(2) => {
                self.diagnostic_mode = !self.diagnostic_mode;
                self.diagnostics.last_mutation =
                    Some(format!("diagnostic_mode={}", self.diagnostic_mode));
                return TuiAction::None;
            }
            _ => {}
        }

        match self.focus {
            Focus::Input => self.handle_input_key(key),
            Focus::Chat => self.handle_chat_key(key),
            Focus::Design => self.handle_design_key(key),
        }
    }

    pub fn enqueue_event(&mut self, event: UiEvent) {
        if let UiEvent::System { summary } = &event {
            crate::tui::render_trace::record(Box::leak(
                format!("[QUEUE_PUSH]\nevent=System\nsummary={summary}").into_boxed_str(),
            ));
        }
        crate::tui::render_trace::record(Box::leak(
            format!(
                "[QUEUE_PUSH] event={:?} queue_len={}",
                event,
                self.event_queue.len() + 1
            )
            .into_boxed_str(),
        ));
        self.event_queue.push(event);
    }

    pub fn handle_ui_events(&mut self) {
        if !self.event_queue.is_empty() {
            crate::tui::render_trace::record(Box::leak(
                format!("[UI_EVENTS] queue_len={}", self.event_queue.len()).into_boxed_str(),
            ));
            self.increment_state_generation();
        }
        while let Some(event) = self.event_queue.pop() {
            if let UiEvent::System { summary } = &event {
                crate::tui::render_trace::record(Box::leak(
                    format!("[QUEUE_PROCESS]\nevent=System\nsummary={summary}").into_boxed_str(),
                ));
            }
            self.append_chat(event);
        }
    }

    pub fn append_chat(&mut self, event: UiEvent) {
        crate::tui::render_trace::record(Box::leak(
            format!("[PROJECTION_UPDATE] state_gen={}", self.state_generation_id).into_boxed_str(),
        ));
        // Phase 4.5: proposal capture and history tracking removed — state lives
        // in Core.  Only UI-side effects (diffs, filter) are applied here.
        self.apply_event_to_session(&event);
        WorkspaceProjector::project(&mut self.workspace, &event);
        self.sync_projection_authority();
        self.chat.append_chat(event);
        self.chat_scroll.apply_append();
        crate::tui::render_trace::record("[RENDER] snapshot_ready");
    }

    pub fn update_design(&mut self, mut new_doc: DesignDocument) {
        if new_doc.version != self.design_doc.version {
            new_doc.regenerate_rendered();
            self.design_doc = new_doc;
            self.design_scroll = self
                .design_scroll
                .min(self.design_doc.rendered.len().saturating_sub(1));
            self.design_updated = true;
        } else {
            self.design_updated = false;
        }
    }

    /// Apply UI-side effects of an event. Projection ownership is bound to
    /// `active_transaction`; render code must not read cached panel state.
    fn apply_event_to_session(&mut self, event: &UiEvent) {
        match event {
            UiEvent::Thinking { summary }
                if summary.contains("processing intent") || summary.contains("queued") =>
            {
                self.runtime_state = RuntimeShellState::Thinking;
            }
            UiEvent::Planning { summary } if summary.contains("planning runtime execution") => {
                self.runtime_state = RuntimeShellState::Plan;
            }
            UiEvent::Execution { step } if step.contains("executing runtime core") => {
                self.runtime_state = RuntimeShellState::Apply;
            }
            UiEvent::Preview { diff } => {
                self.runtime_state = RuntimeShellState::PreviewReady;
                let target = self
                    .active_target
                    .clone()
                    .unwrap_or_else(|| "preview".to_string());
                let preview = Diff {
                    file: target.clone(),
                    changes: diff
                        .iter()
                        .map(|line| DiffChunk {
                            old_line: None,
                            new_line: None,
                            old: None,
                            new: Some(line.clone()),
                        })
                        .collect(),
                };
                self.install_runtime_transaction(target, preview);
            }
            UiEvent::Diff { file, changes } => {
                let Some(existing) = self.active_transaction.as_ref() else {
                    self.clear_runtime_transaction();
                    return;
                };
                let diff = Diff {
                    file: file.clone(),
                    changes: changes.clone(),
                };
                self.install_runtime_transaction(existing.target_path.clone(), diff);
            }
            UiEvent::Debug { message } if message.starts_with("filter set: ") => {
                self.session.filter = message.strip_prefix("filter set: ").map(ToOwned::to_owned);
            }
            UiEvent::Debug { message } if message.contains("\"transaction\"") => {
                self.active_transaction_id = extract_json_string(message, "transaction_id");
                if let (Some(tx), Some(id)) = (
                    self.active_transaction.as_mut(),
                    self.active_transaction_id.clone(),
                ) {
                    tx.tx_id = id;
                }
                self.retain_debug_event("core", message);
            }
            UiEvent::Debug { message } => {
                self.retain_debug_event("core", message);
            }
            UiEvent::Runtime { message } if message.contains("projecting runtime result") => {
                self.runtime_state = RuntimeShellState::Validate;
            }
            UiEvent::System { summary } if summary.contains("completed") => {
                if self.active_transaction.is_none() {
                    self.runtime_state = RuntimeShellState::Idle;
                }
            }
            UiEvent::Pipeline { state } => {
                let next_state = runtime_state_from_pipeline_label(state);
                if matches!(next_state, RuntimeShellState::Idle) {
                    if self.active_transaction.is_some() {
                        self.runtime_state = RuntimeShellState::Idle;
                        self.sync_projection_authority();
                        return;
                    }
                    self.runtime_state = RuntimeShellState::Idle;
                    self.clear_runtime_transaction();
                    return;
                }
                if should_accept_runtime_transition(
                    self.runtime_state,
                    next_state,
                    self.active_transaction.is_some(),
                ) {
                    self.runtime_state = next_state;
                }
            }
            UiEvent::Error { .. } if self.rejection.is_some() => {}
            UiEvent::Error { .. } => {
                let active_tx = self.active_transaction.is_some();
                if should_accept_runtime_transition(
                    self.runtime_state,
                    RuntimeShellState::Failed,
                    active_tx,
                ) {
                    if let Some(tx) = self.active_transaction.as_mut() {
                        tx.failed_recoverable = true;
                        self.runtime_state = RuntimeShellState::Failed;
                    } else {
                        self.runtime_state = RuntimeShellState::Idle;
                        self.clear_runtime_transaction();
                    }
                }
            }
            _ => {}
        }
    }

    fn install_runtime_transaction(&mut self, target_path: String, diff: Diff) {
        if target_path.trim().is_empty() || target_path == "preview" {
            self.rejection = Some(RejectionInfo {
                reason: "unresolved target".to_string(),
                originating_mutation: "install_runtime_transaction".to_string(),
                governance_source: None,
                convergence_source: None,
            });
            self.clear_runtime_transaction();
            self.runtime_state = RuntimeShellState::Rejected;
            return;
        }
        let tx_id = self
            .active_transaction_id
            .clone()
            .unwrap_or_else(|| self.next_transaction_id(&target_path));
        let resolved_target =
            crate::runtime::shell::ResolvedExecutionTarget::from_canonical_path(&target_path);
        self.active_target = Some(target_path.clone());
        self.active_transaction_id = Some(tx_id.clone());
        self.active_transaction = Some(RuntimeTransaction {
            tx_id,
            target_path,
            resolved_target,
            diff,
            failed_recoverable: false,
        });
    }

    fn clear_runtime_transaction(&mut self) {
        if self.active_transaction.is_none() {
            self.active_transaction_id = None;
            self.active_target = None;
            return;
        }
        if runtime_transaction_clear_allowed(self.runtime_state) {
            self.active_transaction = None;
            self.active_transaction_id = None;
            self.active_target = None;
        } else {
            self.sync_projection_authority();
        }
    }

    fn next_transaction_id(&self, target_path: &str) -> String {
        let normalized = target_path
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_ascii_lowercase();
        if normalized.is_empty() || normalized == "preview" {
            "tx-preview".to_string()
        } else {
            format!("tx-{normalized}")
        }
    }

    pub fn flattened_chat_lines(&self) -> Vec<String> {
        let lines = self
            .chat
            .events
            .iter()
            .filter(|event| !matches!(event, UiEvent::Debug { .. }))
            .flat_map(|event| event.lines())
            .filter_map(|line| sanitize_line(&line))
            .collect::<Vec<_>>();
        let Some(filter) = self.session.filter.as_ref() else {
            return lines;
        };
        let token = format!("[{}]", filter.to_ascii_uppercase());
        lines
            .into_iter()
            .filter(|line| line.starts_with(&token))
            .collect()
    }

    /// Lines shown in the design panel.  Phase 4.5: reads from `core_snapshot`.
    pub fn design_panel_lines(&self) -> Vec<String> {
        let design = self.core_snapshot.design.as_ref();
        let version = design.map_or(self.design_doc.version, |d| d.version);
        let score = design.map_or(0.0, |d| d.score());
        let mut lines = vec![
            format!("[DESIGN v{version}]"),
            format!("Score: {score:.2}"),
            format!("[STATE] {}", self.core_snapshot.status.label()),
            String::new(),
        ];
        let summaries: Vec<String> = design
            .map(|d| {
                d.reason_units
                    .iter()
                    .take(5)
                    .map(|u| format!("- {}", u.summary))
                    .collect()
            })
            .unwrap_or_default();
        lines.extend(summaries);
        if lines.len() <= 3 {
            lines.extend(self.design_doc.rendered.iter().cloned());
        }
        lines
    }

    fn handle_input_key(&mut self, key: KeyEvent) -> TuiAction {
        crate::tui::render_trace::record_key_event(key.code, key.modifiers);
        if self.diagnostic_mode {
            self.diagnostics.last_key_event = Some(format!(
                "KEY={:?} MOD={:?} BITS={:#0x}",
                key.code,
                key.modifiers,
                key.modifiers.bits()
            ));
        }
        if Self::is_submit_key(&key) {
            return self.submit_editor();
        }

        match key.code {
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.editor_state.editor.insert_newline();
                if self.diagnostic_mode {
                    self.diagnostics.last_mutation = Some("insert_newline()".to_string());
                }
                TuiAction::None
            }
            KeyCode::Backspace => {
                self.editor_state.editor.backspace();
                if self.diagnostic_mode {
                    self.diagnostics.last_mutation = Some("backspace()".to_string());
                }
                TuiAction::None
            }
            KeyCode::Delete => {
                self.editor_state.editor.delete();
                if self.diagnostic_mode {
                    self.diagnostics.last_mutation = Some("delete()".to_string());
                }
                TuiAction::None
            }
            KeyCode::Left => {
                self.editor_state.editor.move_left();
                TuiAction::None
            }
            KeyCode::Right => {
                self.editor_state.editor.move_right();
                TuiAction::None
            }
            KeyCode::Up => {
                self.editor_state.editor.move_up();
                TuiAction::None
            }
            KeyCode::Down => {
                self.editor_state.editor.move_down();
                TuiAction::None
            }
            KeyCode::Char(ch)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::SUPER) =>
            {
                self.editor_state.editor.insert_char(ch);
                if self.diagnostic_mode {
                    self.diagnostics.last_mutation =
                        Some(format!("insert_char('{}')", ch.escape_debug()));
                }
                TuiAction::None
            }
            _ => TuiAction::None,
        }
    }

    fn is_submit_key(key: &KeyEvent) -> bool {
        let enter = key.code == KeyCode::Enter && !key.modifiers.contains(KeyModifiers::SHIFT);
        let ctrl_d = matches!(key.code, KeyCode::Char('d') | KeyCode::Char('D'))
            && key.modifiers.contains(KeyModifiers::CONTROL);
        // Some terminals expose Ctrl+D as the EOT control character without
        // retaining the CONTROL modifier.
        let eot = key.code == KeyCode::Char('\u{4}');
        enter || ctrl_d || eot
    }

    fn submit_editor(&mut self) -> TuiAction {
        let raw_submitted = self.editor_state.editor.text();
        // Fix-1: 防御的YAML改行復元 — lines配列が単一行に統合された場合の安全網
        let submitted = ensure_yaml_line_breaks(&raw_submitted);
        crate::tui::render_trace::record_payload_dump(
            "RAW_PAYLOAD_BEGIN",
            "RAW_PAYLOAD_END",
            "PAYLOAD",
            &submitted,
        );
        crate::tui::render_trace::record(Box::leak(
            format!(
                "[SUBMIT] input_len={} raw_len={}",
                submitted.len(),
                raw_submitted.len()
            )
            .into_boxed_str(),
        ));
        let trimmed = submitted.trim().to_string();
        if trimmed.is_empty() {
            return TuiAction::None;
        }
        if matches!(trimmed.as_str(), "/exit" | "/quit") {
            self.editor_state.editor.clear();
            return TuiAction::Quit;
        }
        if trimmed == ":diagnostics" {
            self.editor_state.editor.clear();
            self.diagnostic_mode = !self.diagnostic_mode;
            self.diagnostics.last_mutation =
                Some(format!("diagnostic_mode={}", self.diagnostic_mode));
            return TuiAction::None;
        }
        if trimmed == "/save design" {
            self.history.push(trimmed);
            self.history_cursor = None;
            self.editor_state.editor.clear();
            return TuiAction::SaveDesign;
        }
        self.record_history(trimmed.clone());
        self.history_cursor = None;
        self.update_runtime_intent_state(&trimmed);
        if self.diagnostic_mode {
            // Fix-2: Diagnostics表示で行数情報を追加
            self.diagnostics.last_mutation = Some(format!(
                "submit('{}') lines={}",
                trimmed.lines().next().unwrap_or(""),
                trimmed.lines().count()
            ));
        }
        crate::tui::render_trace::record(Box::leak(
            format!("[SUBMIT_ACTION_CREATED] input_len={}", submitted.len()).into_boxed_str(),
        ));
        TuiAction::Submit(submitted)
    }

    fn handle_chat_key(&mut self, key: KeyEvent) -> TuiAction {
        match key.code {
            KeyCode::PageUp | KeyCode::Up => {
                self.chat_scroll.user_scroll_up(5);
                if self.diagnostic_mode {
                    self.diagnostics.last_mutation = Some("chat_scroll_up".to_string());
                }
            }
            KeyCode::PageDown | KeyCode::Down => {
                self.chat_scroll.user_scroll_down(5);
                if self.diagnostic_mode {
                    self.diagnostics.last_mutation = Some("chat_scroll_down".to_string());
                }
            }
            KeyCode::End => {
                self.chat_scroll.scroll_to_bottom();
                if self.diagnostic_mode {
                    self.diagnostics.last_mutation = Some("chat_scroll_bottom".to_string());
                }
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                self.narrative_expanded = !self.narrative_expanded;
                if self.diagnostic_mode {
                    self.diagnostics.last_mutation =
                        Some(format!("toggle_narrative({})", self.narrative_expanded));
                }
            }
            _ => {}
        }
        TuiAction::None
    }

    fn handle_design_key(&mut self, key: KeyEvent) -> TuiAction {
        match key.code {
            KeyCode::PageUp | KeyCode::Up => {
                self.design_scroll = self.design_scroll.saturating_sub(5);
                if self.diagnostic_mode {
                    self.diagnostics.last_mutation = Some("design_scroll_up".to_string());
                }
            }
            KeyCode::PageDown | KeyCode::Down => {
                let max = self.design_doc.rendered.len().saturating_sub(1);
                self.design_scroll = (self.design_scroll + 5).min(max);
                if self.diagnostic_mode {
                    self.diagnostics.last_mutation = Some("design_scroll_down".to_string());
                }
            }
            KeyCode::Home => {
                self.design_scroll = 0;
                if self.diagnostic_mode {
                    self.diagnostics.last_mutation = Some("design_scroll_home".to_string());
                }
            }
            KeyCode::End => {
                self.design_scroll = self.design_doc.rendered.len().saturating_sub(1);
                if self.diagnostic_mode {
                    self.diagnostics.last_mutation = Some("design_scroll_end".to_string());
                }
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                self.design_collapsed = !self.design_collapsed;
                if self.diagnostic_mode {
                    self.diagnostics.last_mutation = Some(format!(
                        "toggle_design_collapsed({})",
                        self.design_collapsed
                    ));
                }
            }
            _ => {}
        }
        TuiAction::None
    }

    fn record_history(&mut self, submitted: String) {
        self.history.push(submitted.clone());
        if let Some(store) = self.persistent_history.as_ref() {
            let _ = store.append(&submitted);
        }
    }

    fn update_runtime_intent_state(&mut self, submitted: &str) {
        self.language_mode = detect_runtime_language(submitted);
        if let Some(normalized) = normalize_runtime_input(submitted) {
            self.language_mode = normalized.language;
        }
    }

    fn retain_debug_event(&mut self, source: &str, message: &str) {
        self.debug_events.push(DebugEvent {
            timestamp: 0,
            source: source.to_string(),
            message: message.to_string(),
            level: crate::runtime::runtime_events::DebugLevel::Debug,
        });
        const MAX_DEBUG_EVENTS: usize = 200;
        if self.debug_events.len() > MAX_DEBUG_EVENTS {
            let overflow = self.debug_events.len() - MAX_DEBUG_EVENTS;
            self.debug_events.drain(..overflow);
        }
    }

    fn with_pseudo_stream(mut self) -> Self {
        for event in pseudo_stream_events() {
            self.enqueue_event(event);
        }
        self
    }
}

fn seed_chat_stream(payload: &UiPayload) -> Vec<UiEvent> {
    let mut events = Vec::new();

    // DBM-CLI Branding Integration: Startup narratives
    events.push(UiEvent::Result {
        message: "Initializing cognitive runtime.".to_string(),
    });
    events.push(UiEvent::Result {
        message: "認知ランタイムを初期化しています。".to_string(),
    });
    events.push(UiEvent::Result {
        message: "Governed cognitive runtime is ready.".to_string(),
    });
    events.push(UiEvent::Result {
        message: "認知実行ランタイムの準備が完了しました。".to_string(),
    });

    events.push(UiEvent::Pipeline {
        state: format!("request_id={}", payload.trace.request_id),
    });

    for step in &payload.trace.steps {
        events.push(UiEvent::Thinking {
            summary: format!(
                "depth={} beam={} candidates={} pruned={} recall_hits={}",
                step.depth, step.beam_width, step.candidates, step.pruned, step.recall_hits
            ),
        });
    }

    if let Some(selected) = payload.selected {
        events.push(UiEvent::Result {
            message: format!("selected hypothesis H{selected}"),
        });
    }

    if events.len() > MAX_CHAT_LINES {
        events.drain(0..events.len() - MAX_CHAT_LINES);
    }
    events
}

pub fn pseudo_stream_events() -> Vec<UiEvent> {
    vec![
        UiEvent::Thinking {
            summary: "analyzing".to_string(),
        },
        UiEvent::Editing {
            target: "parser".to_string(),
            action: "replace block".to_string(),
        },
        UiEvent::Result {
            message: "done".to_string(),
        },
    ]
}

fn seed_design_document(payload: &UiPayload) -> DesignDocument {
    let reason_units = payload
        .hypotheses
        .iter()
        .take(8)
        .map(|hyp| ReasonUnit {
            id: format!("H{}", hyp.id),
            title: format!("H{}", hyp.id),
            summary: format!("depth={} score={:.2}", hyp.depth, hyp.score),
        })
        .collect();

    DesignDocument::new(
        1,
        reason_units,
        StructureTree {
            module: "runtime_design".to_string(),
            functions: vec![
                "design convergence view".to_string(),
                "chat stream append".to_string(),
                "input buffer".to_string(),
            ],
        },
        vec![
            Constraint {
                text: "Core independent".to_string(),
            },
            Constraint {
                text: "append-only chat buffer".to_string(),
            },
            Constraint {
                text: "max design rows 20".to_string(),
            },
        ],
    )
}

fn runtime_state_from_pipeline_label(label: &str) -> RuntimeShellState {
    match label {
        "Proposed" => RuntimeShellState::Plan,
        "Planned" => RuntimeShellState::Validate,
        "Previewed" => RuntimeShellState::AwaitConfirmation,
        "Applied" => RuntimeShellState::Apply,
        "Staged" | "Committed" => RuntimeShellState::Git,
        "Idle" => RuntimeShellState::Idle,
        _ => RuntimeShellState::Idle,
    }
}

fn runtime_transaction_clear_allowed(state: RuntimeShellState) -> bool {
    matches!(
        state,
        RuntimeShellState::Git
            | RuntimeShellState::Rejected
            | RuntimeShellState::GovernanceRejected
            | RuntimeShellState::SemanticRejected
            | RuntimeShellState::ConvergenceRejected
            | RuntimeShellState::SemanticDriftRejected
            | RuntimeShellState::ExecutionRejected
            | RuntimeShellState::RemoteExecutionRejected
    ) || state.label().contains("HALT")
}

pub fn should_accept_runtime_transition(
    current: RuntimeShellState,
    next: RuntimeShellState,
    active_tx: bool,
) -> bool {
    if active_tx && current == RuntimeShellState::PreviewReady {
        return next == RuntimeShellState::PreviewReady;
    }
    true
}

fn extract_json_string(input: &str, key: &str) -> Option<String> {
    let needle = format!("\"{key}\":\"");
    let start = input.find(&needle)? + needle.len();
    let rest = &input[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::tui::model::{ScorePartsViewModel, TraceStatsViewModel, TraceViewModel, UiPayload};
    use crate::tui::render_trace;

    fn empty_payload() -> UiPayload {
        UiPayload {
            trace: TraceViewModel {
                request_id: "test".to_string(),
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

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn design_doc(version: u64, module: &str) -> DesignDocument {
        DesignDocument::new(
            version,
            vec![ReasonUnit {
                id: "ru-1".to_string(),
                title: "parser".to_string(),
                summary: "parse input".to_string(),
            }],
            StructureTree {
                module: module.to_string(),
                functions: vec!["parse_input".to_string()],
            },
            vec![Constraint {
                text: "no unsafe unwrap".to_string(),
            }],
        )
    }

    fn runtime_transaction(target: &str) -> RuntimeTransaction {
        RuntimeTransaction {
            tx_id: "tx-projection-authority".to_string(),
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
    fn projection_sync_preserves_active_target() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = None;
        state.active_transaction = Some(runtime_transaction("apps/cli/src/repl.rs"));

        state.sync_projection_authority();

        assert_eq!(state.active_target.as_deref(), Some("apps/cli/src/repl.rs"));
    }

    #[test]
    fn focus_cycles_forward_and_backward() {
        let mut state = TuiState::new(empty_payload());
        assert_eq!(state.focus, Focus::Input);

        state.handle_key_event(key(KeyCode::Tab));
        assert_eq!(state.focus, Focus::Chat);

        state.handle_key_event(key(KeyCode::Tab));
        assert_eq!(state.focus, Focus::Design);

        state.handle_key_event(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(state.focus, Focus::Chat);
    }

    #[test]
    fn editor_command_enter_submits_and_records_history() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        for ch in "fix parser bug".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        let action = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SUPER));

        assert_eq!(action, TuiAction::Submit("fix parser bug".to_string()));
        assert_eq!(state.history, vec!["fix parser bug"]);
        assert_eq!(state.editor_state.editor.text(), "fix parser bug");
    }

    #[test]
    fn editor_command_enter_preserves_multiline_submitted_payload() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        let payload =
            "system_name: DBM_TUI_Test\n\nrules:\n  - Runtime must pass through ApplyGate";
        for ch in payload.chars() {
            if ch == '\n' {
                state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
            } else {
                state.handle_key_event(key(KeyCode::Char(ch)));
            }
        }

        let action = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SUPER));

        assert_eq!(action, TuiAction::Submit(payload.to_string()));
        assert_ne!(
            action,
            TuiAction::Submit(
                "system_name: DBM_TUI_Testrules:_Runtime must pass through ApplyGate".to_string()
            )
        );
    }

    #[test]
    fn editor_tab_changes_focus_without_command_completion() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        for ch in "git s".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        let action = state.handle_key_event(key(KeyCode::Tab));

        assert_eq!(action, TuiAction::None);
        assert_eq!(state.editor_state.editor.text(), "git s");
        assert_eq!(state.focus, Focus::Chat);
    }

    #[test]
    fn persistent_history_loads_and_appends() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join(".dbm/cli_history");
        let mut state = TuiState::new(empty_payload());
        state.enable_persistent_history(path.clone());
        state.editor_state.editor.clear();
        for ch in "preview parser.rs".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        let action = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SUPER));

        assert_eq!(action, TuiAction::Submit("preview parser.rs".to_string()));
        assert_eq!(
            std::fs::read_to_string(path).expect("history"),
            "preview parser.rs\n"
        );
    }

    #[test]
    fn save_design_command_is_ui_action() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        for ch in "/save design".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        let action = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SUPER));

        assert_eq!(action, TuiAction::SaveDesign);
        assert_eq!(state.history, vec!["/save design"]);
    }

    /// Projection diffs are owned by the runtime publication gate. A raw diff
    /// event cannot create transaction ownership by itself.
    #[test]
    fn raw_diff_without_transaction_does_not_publish_projection() {
        let mut state = TuiState::new(empty_payload());

        state.append_chat(UiEvent::Proposal { candidates: vec![] });
        state.append_chat(UiEvent::Plan {
            steps: vec!["Fix parser.rs".to_string()],
        });
        state.append_chat(UiEvent::DesignUpdate {
            summary: "Parser modularized".to_string(),
            score: 0.82,
        });
        assert!(state.active_transaction.is_none());

        state.append_chat(UiEvent::Diff {
            file: "parser.rs".to_string(),
            changes: vec![DiffChunk {
                old_line: Some(1),
                new_line: Some(1),
                old: Some("fn parse()".to_string()),
                new: Some("fn parse(input: &str)".to_string()),
            }],
        });
        assert!(state.active_transaction.is_none());
        assert!(state.active_transaction_id.is_none());
        assert!(state.active_target.is_none());
    }

    #[test]
    fn preview_diff_label_renders_previewed_not_applied() {
        let text = UiEvent::Diff {
            file: "Cargo.toml".to_string(),
            changes: vec![DiffChunk {
                old_line: Some(1),
                new_line: Some(1),
                old: Some("version = \"0.1.0\"".to_string()),
                new: Some("version = \"0.1.1\"".to_string()),
            }],
        }
        .text();

        assert!(text.contains("Previewed"));
        assert!(!text.contains("Applied"));
        assert!(text.contains("Cargo.toml"));
    }

    #[test]
    fn pipeline_idle_preserves_projection_lifecycle_while_transaction_active() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("parser.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["+fn parse() {}".to_string()],
        });
        assert!(state.active_transaction.is_some());

        state.append_chat(UiEvent::Pipeline {
            state: "Idle".to_string(),
        });

        assert!(state.active_transaction.is_some());
        assert!(state.active_transaction_id.is_some());
        assert_eq!(state.runtime_state, RuntimeShellState::Idle);
        assert_eq!(state.active_target.as_deref(), Some("parser.rs"));
    }

    #[test]
    fn preview_then_idle_projection_then_apply_preserves_transaction() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("apps/cli/src/repl.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["+fn validate_runtime() {}".to_string()],
        });
        let before_tx = state.active_transaction.clone();
        state.active_target = None;

        state.append_chat(UiEvent::Pipeline {
            state: "Idle".to_string(),
        });

        assert_eq!(state.runtime_state, RuntimeShellState::Idle);
        assert_eq!(state.active_transaction, before_tx);
        assert_eq!(state.active_target.as_deref(), Some("apps/cli/src/repl.rs"));
    }

    #[test]
    fn runtime_projected_does_not_clear_active_transaction() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("apps/cli/src/repl.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["+fn validate_runtime() {}".to_string()],
        });
        let tx_id = state.active_transaction_id.clone();

        state.append_chat(UiEvent::Pipeline {
            state: "Previewed".to_string(),
        });

        assert!(state.active_transaction.is_some());
        assert_eq!(state.active_transaction_id, tx_id);
        assert_eq!(state.active_target.as_deref(), Some("apps/cli/src/repl.rs"));
    }

    #[test]
    fn diff_updates_existing_projection_without_claiming_target() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("parser.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["+fn parse() {}".to_string()],
        });

        state.append_chat(UiEvent::Diff {
            file: "other.rs".to_string(),
            changes: vec![DiffChunk {
                old_line: None,
                new_line: Some(1),
                old: None,
                new: Some("fn parse(input: &str) {}".to_string()),
            }],
        });

        let tx = state
            .active_transaction
            .as_ref()
            .expect("active transaction");
        assert_eq!(tx.target_path, "parser.rs");
        assert_eq!(tx.diff.file, "other.rs");
    }

    #[test]
    fn failed_without_transaction_clears_projection() {
        let mut state = TuiState::new(empty_payload());

        state.append_chat(UiEvent::Error {
            message: "failed before preview".to_string(),
        });

        assert_eq!(state.runtime_state, RuntimeShellState::Idle);
        assert!(state.active_transaction.is_none());
    }

    #[test]
    fn failed_recoverable_requires_transaction() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("parser.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["+fn parse() {}".to_string()],
        });
        state.append_chat(UiEvent::Diff {
            file: "parser.rs".to_string(),
            changes: vec![DiffChunk {
                old_line: None,
                new_line: Some(1),
                old: None,
                new: Some("fn parse() {}".to_string()),
            }],
        });
        state.runtime_state = RuntimeShellState::Apply;

        state.append_chat(UiEvent::Error {
            message: "apply failed".to_string(),
        });

        let tx = state.active_transaction.as_ref().expect("recoverable tx");
        assert!(tx.failed_recoverable);
        assert!(tx.tx_id.starts_with("tx-"));
        assert_eq!(state.runtime_state, RuntimeShellState::Failed);
    }

    /// Phase 4.5: `core_snapshot` is the SSOT; UI can assign it directly to
    /// simulate Core returning a restored state (e.g. after undo).
    #[test]
    fn core_snapshot_reflects_core_state_after_assignment() {
        use crate::core::{CorePlan, CoreState};

        let mut state = TuiState::new(empty_payload());
        assert_eq!(state.core_snapshot.status, PipelineState::Idle);

        // Simulate Core returning a Proposed snapshot (e.g. after proposal).
        state.core_snapshot = CoreState {
            version: 1,
            status: PipelineState::Proposed,
            ..CoreState::default()
        };
        assert_eq!(state.core_snapshot.status, PipelineState::Proposed);

        // Simulate Core returning a Planned snapshot (e.g. after select).
        state.core_snapshot = CoreState {
            version: 2,
            status: PipelineState::Planned,
            current_plan: Some(CorePlan {
                summary: "Fix parser.rs".to_string(),
                steps: vec!["fix parser.rs".to_string()],
            }),
            ..CoreState::default()
        };
        assert_eq!(state.core_snapshot.status, PipelineState::Planned);
        assert_eq!(
            state
                .core_snapshot
                .current_plan
                .as_ref()
                .map(|p| p.summary.as_str()),
            Some("Fix parser.rs")
        );

        // Undo: Core returns a restored snapshot — UI just sets core_snapshot.
        state.core_snapshot = CoreState {
            version: 1,
            status: PipelineState::Proposed,
            ..CoreState::default()
        };
        assert_eq!(state.core_snapshot.status, PipelineState::Proposed);
        assert_eq!(state.core_snapshot.version, 1);
    }

    #[test]
    fn filter_event_limits_flattened_chat_lines() {
        let mut state = TuiState::new(empty_payload());
        state.append_chat(UiEvent::Execution {
            step: "run".to_string(),
        });
        state.append_chat(UiEvent::Diff {
            file: "parser.rs".to_string(),
            changes: vec![DiffChunk {
                old_line: Some(1),
                new_line: Some(1),
                old: Some("fn parse()".to_string()),
                new: Some("fn parse(input: &str)".to_string()),
            }],
        });
        state.append_chat(UiEvent::Debug {
            message: "filter set: diff".to_string(),
        });

        let lines = state.flattened_chat_lines();
        assert!(!lines.is_empty());
        assert!(lines.iter().all(|line| line.starts_with("[DIFF]")));
    }

    #[test]
    fn runtime_status_tracks_target_language_and_state() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        for ch in "parser.rs を preview".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        let action = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SUPER));

        assert_eq!(
            action,
            TuiAction::Submit("parser.rs を preview".to_string())
        );
        assert_eq!(state.active_target.as_deref(), None);
        assert_eq!(state.language_mode, SupportedLanguage::Japanese);
        assert_eq!(state.runtime_state, RuntimeShellState::Idle);
    }

    #[test]
    fn chat_buffer_is_capped() {
        let mut state = TuiState::new(empty_payload());
        for idx in 0..(MAX_CHAT_LINES + 10) {
            state.append_chat(UiEvent::Thinking {
                summary: format!("event {idx}"),
            });
        }

        assert_eq!(state.chat.events.len(), MAX_CHAT_LINES);
        assert_eq!(
            state.chat.events.first().map(UiEvent::text),
            Some("event 10".to_string())
        );
    }

    #[test]
    fn shift_enter_inserts_newlines_in_editor() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        state.handle_key_event(key(KeyCode::Char('a')));
        for _ in 0..4 {
            state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
            state.handle_key_event(key(KeyCode::Char('b')));
        }

        assert_eq!(state.editor_state.editor.line_count(), 5);
    }

    #[test]
    fn plain_enter_submits_without_terminal_modifier_support() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        for ch in "fix parser bug".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        let action = state.handle_key_event(key(KeyCode::Enter));

        assert_eq!(action, TuiAction::Submit("fix parser bug".to_string()));
    }

    #[test]
    fn shifted_printable_characters_insert_as_terminal_generated_text() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();

        for ch in ['a', 'A', '!', '{', '}'] {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        assert_eq!(state.editor_state.editor.text(), "aA!{}");
    }

    #[test]
    fn diagnostics_records_shift_modified_physical_key_events() {
        let mut state = TuiState::new(empty_payload());
        state.diagnostic_mode = true;
        state.editor_state.editor.clear();

        state.handle_key_event(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::SHIFT));

        assert_eq!(state.editor_state.editor.text(), "1");
        assert!(
            state
                .diagnostics
                .last_key_event
                .as_deref()
                .is_some_and(|event| event.contains("KEY=Char('1')") && event.contains("SHIFT"))
        );
        assert_eq!(
            state.diagnostics.last_mutation.as_deref(),
            Some("insert_char('1')")
        );
    }

    #[test]
    fn ctrl_d_submits_editor_as_fallback() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        for ch in "fix parser bug".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        let action =
            state.handle_key_event(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));

        assert_eq!(action, TuiAction::Submit("fix parser bug".to_string()));
        assert_eq!(state.history, vec!["fix parser bug"]);
    }

    #[test]
    fn eot_control_character_submits_editor_as_ctrl_d_fallback() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        for ch in "fix parser bug".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        let action =
            state.handle_key_event(KeyEvent::new(KeyCode::Char('\u{4}'), KeyModifiers::NONE));

        assert_eq!(action, TuiAction::Submit("fix parser bug".to_string()));
    }

    #[test]
    fn ctrl_enter_submits_editor_as_fallback() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        for ch in "fix parser bug".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        let action = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));

        assert_eq!(action, TuiAction::Submit("fix parser bug".to_string()));
        assert_eq!(state.history, vec!["fix parser bug"]);
    }

    #[test]
    fn shifted_enter_is_reserved_for_newline() {
        let mut command_state = TuiState::new(empty_payload());
        command_state.editor_state.editor.clear();
        for ch in "command submit".chars() {
            command_state.handle_key_event(key(KeyCode::Char(ch)));
        }
        let command_action = command_state.handle_key_event(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::SUPER | KeyModifiers::SHIFT,
        ));
        assert_eq!(command_action, TuiAction::None);
        assert_eq!(command_state.editor_state.editor.line_count(), 2);

        let mut ctrl_state = TuiState::new(empty_payload());
        ctrl_state.editor_state.editor.clear();
        for ch in "control submit".chars() {
            ctrl_state.handle_key_event(key(KeyCode::Char(ch)));
        }
        let ctrl_action = ctrl_state.handle_key_event(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        ));
        assert_eq!(ctrl_action, TuiAction::None);
        assert_eq!(ctrl_state.editor_state.editor.line_count(), 2);
    }

    #[test]
    fn input_key_events_are_recorded_to_trace_buffer() {
        render_trace::reset();
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();

        state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));
        state.handle_key_event(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));

        let events = render_trace::key_event_snapshot();
        assert!(events.iter().any(|event| {
            event.contains("KEY=Enter") && event.contains("MOD=") && event.contains("CONTROL")
        }));
        assert!(events.iter().any(|event| {
            event.contains("KEY=Char('d')") && event.contains("MOD=") && event.contains("CONTROL")
        }));
    }

    #[test]
    fn command_q_quits_tui() {
        let mut state = TuiState::new(empty_payload());

        let action = state.handle_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::SUPER));

        assert_eq!(action, TuiAction::Quit);
    }

    #[test]
    fn ctrl_q_no_longer_quits_tui() {
        let mut state = TuiState::new(empty_payload());

        let action =
            state.handle_key_event(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL));

        assert_eq!(action, TuiAction::None);
    }

    #[test]
    fn editor_arrow_keys_move_cursor_across_lines() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        for ch in "abc".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }
        state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
        for ch in "de".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        state.handle_key_event(key(KeyCode::Up));
        assert_eq!(state.editor_state.editor.cursor_row, 0);
        assert_eq!(state.editor_state.editor.cursor_col, 2);

        state.handle_key_event(key(KeyCode::Down));
        assert_eq!(state.editor_state.editor.cursor_row, 1);
        assert_eq!(state.editor_state.editor.cursor_col, 2);

        state.handle_key_event(key(KeyCode::Left));
        assert_eq!(state.editor_state.editor.cursor_col, 1);
        state.handle_key_event(key(KeyCode::Right));
        assert_eq!(state.editor_state.editor.cursor_col, 2);
    }

    #[test]
    fn editor_backspace_at_line_start_joins_previous_line() {
        let mut editor = SpecificationEditor {
            lines: vec!["abc".to_string(), "def".to_string()],
            cursor_row: 1,
            cursor_col: 0,
        };

        editor.backspace();

        assert_eq!(editor.lines, vec!["abcdef".to_string()]);
        assert_eq!(editor.cursor_row, 0);
        assert_eq!(editor.cursor_col, 3);
    }

    #[test]
    fn esc_does_not_clear_editor_contents() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        for ch in "hello".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        let action = state.handle_key_event(key(KeyCode::Esc));

        assert_eq!(action, TuiAction::None);
        assert_eq!(state.editor_state.editor.lines, vec!["hello".to_string()]);
    }

    #[test]
    fn esc_returns_none_in_input_focus() {
        let mut state = TuiState::new(empty_payload());
        assert_eq!(state.focus, Focus::Input);

        let action = state.handle_key_event(key(KeyCode::Esc));

        assert_eq!(action, TuiAction::None);
    }

    #[test]
    fn shift_enter_compatibility_does_not_destroy_editor() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        for ch in "design spec".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        // Ghostty / Crossterm で Shift+Enter が Esc として観測されるケース
        let esc_event = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = state.handle_key_event(esc_event);

        assert_eq!(action, TuiAction::None);
        assert_eq!(
            state.editor_state.editor.lines,
            vec!["design spec".to_string()]
        );
    }

    #[test]
    fn design_specification_survives_escape_event() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        let spec = "system_name: \"DBM\"\ngoals:\n  - Analyze architecture";
        for ch in spec.chars() {
            if ch == '\n' {
                state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
            } else {
                state.handle_key_event(key(KeyCode::Char(ch)));
            }
        }
        let lines_before = state.editor_state.editor.lines.clone();

        state.handle_key_event(key(KeyCode::Esc));

        assert_eq!(state.editor_state.editor.lines, lines_before);
    }

    #[test]
    fn editor_accepts_large_specification_until_max_lines() {
        let mut editor = SpecificationEditor {
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
        };

        for _ in 1..(MAX_SPEC_LINES + 10) {
            editor.insert_newline();
        }

        assert_eq!(editor.lines.len(), MAX_SPEC_LINES);
    }

    #[test]
    fn design_document_update_uses_semantic_structure_and_rerenders() {
        let mut state = TuiState::new(empty_payload());
        let version = state.design_doc.version;

        state.update_design(design_doc(version + 1, "parser"));

        assert_eq!(state.design_doc.version, version + 1);
        assert_eq!(state.design_doc.structure.module, "parser");
        assert!(
            state
                .design_doc
                .rendered
                .iter()
                .any(|line| line == "Module: parser")
        );
        assert!(state.design_updated);
    }

    #[test]
    fn design_document_same_version_does_not_update() {
        let mut state = TuiState::new(empty_payload());
        let version = state.design_doc.version;
        state.update_design(design_doc(version, "parser"));

        assert_eq!(state.design_doc.version, version);
        assert_ne!(state.design_doc.structure.module, "parser");
        assert!(!state.design_updated);
    }

    #[test]
    fn design_document_version_change_rerenders_even_when_semantics_match() {
        let mut state = TuiState::new(empty_payload());
        let mut doc = state.design_doc.clone();
        doc.version += 1;
        doc.rendered = vec!["stale".to_string()];
        let version = state.design_doc.version;

        state.update_design(doc);

        assert_eq!(state.design_doc.version, version + 1);
        assert_ne!(state.design_doc.rendered, vec!["stale".to_string()]);
        assert!(state.design_updated);
    }

    #[test]
    fn event_queue_is_fifo_and_drops_oldest_over_limit() {
        let mut queue = EventQueue::default();
        for idx in 0..(MAX_EVENTS + 3) {
            queue.push(UiEvent::Debug {
                message: format!("event {idx}"),
            });
        }

        assert_eq!(queue.len(), MAX_EVENTS);
        assert_eq!(
            queue.pop().map(|event| event.text()),
            Some("event 3".to_string())
        );
        assert_eq!(
            queue.pop().map(|event| event.text()),
            Some("event 4".to_string())
        );
    }

    #[test]
    fn pseudo_stream_flows_through_queue_in_order() {
        let mut state = TuiState::new(empty_payload());

        state.handle_ui_events();

        let lines = state.flattened_chat_lines();
        let pseudo_lines: Vec<String> = lines
            .iter()
            .filter(|line| {
                line == &&"[THINKING] analyzing".to_string()
                    || line == &&"[EDITING] parser: replace block".to_string()
                    || line == &&"[RESULT] done".to_string()
            })
            .cloned()
            .collect();
        assert_eq!(
            pseudo_lines,
            vec![
                "[THINKING] analyzing".to_string(),
                "[EDITING] parser: replace block".to_string(),
                "[RESULT] done".to_string(),
            ]
        );
    }

    #[test]
    fn chat_scroll_state_transitions_are_stable() {
        let mut scroll = ChatScrollState::default();
        assert!(scroll.is_following);

        scroll.user_scroll_up(5);
        assert!(!scroll.is_following);
        assert_eq!(scroll.offset, 5);

        scroll.apply_append();
        assert_eq!(scroll.offset, 5);
        assert!(!scroll.is_following);

        scroll.user_scroll_down(5);
        assert_eq!(scroll.offset, 0);
        assert!(scroll.is_following);
    }

    #[test]
    fn queued_event_does_not_interfere_with_editor_buffer() {
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        for ch in "typing".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        state.enqueue_event(UiEvent::Thinking {
            summary: "async event".to_string(),
        });
        state.handle_ui_events();

        assert_eq!(state.editor_state.editor.text(), "typing");
        assert!(
            state
                .flattened_chat_lines()
                .iter()
                .any(|line| line == "[THINKING] async event")
        );
    }

    #[test]
    fn appending_while_scrolled_keeps_fixed_offset() {
        let mut state = TuiState::new(empty_payload());
        state.focus = Focus::Chat;
        state.handle_key_event(key(KeyCode::PageUp));

        state.enqueue_event(UiEvent::Thinking {
            summary: "async event".to_string(),
        });
        state.handle_ui_events();

        assert_eq!(state.chat_scroll.offset, 5);
        assert!(!state.chat_scroll.is_following);
    }

    #[test]
    fn seed_design_document_limits_rows() {
        let mut payload = empty_payload();
        payload.hypotheses = (0..50)
            .map(|id| crate::tui::model::HypothesisViewModel {
                id,
                parent: None,
                depth: id,
                score: 0.5,
                score_parts: ScorePartsViewModel {
                    relevance: 0.0,
                    goal: 0.0,
                    constraint: 0.0,
                    memory: 0.0,
                },
                relations: vec![],
            })
            .collect();

        let state = TuiState::new(payload);

        assert!(state.design_doc.rendered.len() <= DESIGN_MAX_LINES);
    }

    #[test]
    fn deterministic_runtime_text_rendering_is_stable() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("parser.rs".to_string());
        state.runtime_state = RuntimeShellState::PreviewReady;
        state.append_chat(UiEvent::Preview {
            diff: vec!["+fn parse() {}".to_string()],
        });

        let first = crate::tui::rendering::runtime_semantic_events(&state);
        let second = crate::tui::rendering::runtime_semantic_events(&state);

        assert_eq!(first, second);
        assert!(
            first
                .iter()
                .any(|event| event.render().contains("preview ready"))
        );
    }

    #[test]
    fn preview_ready_cannot_be_overwritten_by_old_applying() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("parser.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["+fn parse() {}".to_string()],
        });

        state.append_chat(UiEvent::Pipeline {
            state: "Applied".to_string(),
        });

        assert_eq!(state.runtime_state, RuntimeShellState::PreviewReady);
    }

    #[test]
    fn failed_preview_preserves_runtime_state() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("apps/cli/src/core.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["+preview apps/cli/src/core.rs".to_string()],
        });
        let before_state = state.runtime_state;
        let before_tx = state.active_transaction.clone();
        let before_target = state.active_target.clone();

        state.append_chat(UiEvent::Pipeline {
            state: "Proposed".to_string(),
        });
        state.append_chat(UiEvent::Error {
            message: "failed invalid preview".to_string(),
        });

        assert_eq!(state.runtime_state, before_state);
        assert_eq!(state.active_transaction, before_tx);
        assert_eq!(state.active_target, before_target);
    }

    #[test]
    fn preview_ready_cannot_be_overwritten_by_analyze() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("apps/cli/src/core.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["+preview apps/cli/src/core.rs".to_string()],
        });

        state.append_chat(UiEvent::Pipeline {
            state: "Proposed".to_string(),
        });

        assert_eq!(state.runtime_state, RuntimeShellState::PreviewReady);
    }

    #[test]
    fn intent_prediction_never_mutates_runtime_state() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("apps/cli/src/core.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["+preview apps/cli/src/core.rs".to_string()],
        });
        let before_state = state.runtime_state;
        let before_target = state.active_target.clone();
        let before_tx = state.active_transaction.clone();

        state.update_runtime_intent_state("preview does/not/exist.rs");

        assert_eq!(state.runtime_state, before_state);
        assert_eq!(state.active_target, before_target);
        assert_eq!(state.active_transaction, before_tx);
    }

    #[test]
    fn shell_commit_is_runtime_authority() {
        let root = tempfile::tempdir().expect("tempdir");
        let target = root.path().join("core.rs");
        std::fs::write(&target, "fn core() {}\n").expect("write");
        let mut state = TuiState::new(empty_payload());
        state.update_runtime_intent_state("preview core.rs");
        assert_eq!(state.runtime_state, RuntimeShellState::Idle);

        crate::runtime::shell::runtime_preview(&mut state, root.path(), "core.rs".into());

        assert_eq!(state.runtime_state, RuntimeShellState::PreviewReady);
        assert!(state.active_transaction.is_some());
    }

    #[test]
    fn stale_pipeline_event_never_overwrites_preview() {
        let mut state = TuiState::new(empty_payload());
        state.active_target = Some("apps/cli/src/core.rs".to_string());
        state.append_chat(UiEvent::Preview {
            diff: vec!["+preview apps/cli/src/core.rs".to_string()],
        });
        let before = state.runtime_state;

        for pipeline_state in ["Proposed", "Planned", "Previewed", "Applied"] {
            state.append_chat(UiEvent::Pipeline {
                state: pipeline_state.to_string(),
            });
            assert_eq!(state.runtime_state, before, "{pipeline_state}");
        }
    }

    // ---- Fix-3: backspace() YAML構造保護テスト ----

    #[test]
    fn backspace_preserves_yaml_key_boundary() {
        let mut editor = SpecificationEditor {
            lines: vec![
                "system_name: \"\"".to_string(),
                String::new(),
                "goals:".to_string(),
            ],
            cursor_row: 0,
            cursor_col: 0,
        };
        editor.cursor_row = 2;
        editor.cursor_col = 0;

        // Backspace: goals 行を直前の空行に結合しようとするが、
        // その前の YAML 文脈は保持されるべき
        editor.backspace();

        assert_eq!(editor.lines[0], "system_name: \"\"");
        assert!(editor.lines.iter().any(|line| line == "goals:"));
    }

    #[test]
    fn backspace_allows_merge_for_non_yaml_key_lines() {
        let mut editor = SpecificationEditor {
            lines: vec!["hello world".to_string(), " continued".to_string()],
            cursor_row: 1,
            cursor_col: 0,
        };

        editor.backspace();

        assert_eq!(editor.lines.len(), 1);
        assert_eq!(editor.lines[0], "hello world continued");
    }

    #[test]
    fn backspace_on_yaml_key_with_trailing_spaces_still_protected() {
        let mut editor = SpecificationEditor {
            lines: vec!["goals:  ".to_string(), "  - first_goal".to_string()],
            cursor_row: 1,
            cursor_col: 0,
        };

        editor.backspace();

        // "goals:  " は trim_end() すると "goals:" で ':' 終端なので保護される
        assert_eq!(editor.lines.len(), 2);
        assert_eq!(editor.lines[0], "goals:  ");
        assert_eq!(editor.lines[1], "  - first_goal");
    }

    // ---- Fix-1: ensure_yaml_line_breaks テスト ----

    #[test]
    fn ensure_yaml_line_breaks_restores_missing_newlines() {
        let input = "system_name: testtestgoals:  - verify_runtimeconstraints:  - no_side_effects";
        let restored = ensure_yaml_line_breaks(input);

        assert!(restored.contains('\n'), "改行が復元されるべき");
        assert!(restored.contains("system_name: testtest\ngoals:"));
        assert!(restored.contains("goals: - verify_runtime\nconstraints:"));
    }

    #[test]
    fn ensure_yaml_line_breaks_preserves_existing_newlines() {
        let input = "system_name: testtest\n\ngoals:\n  - verify_runtime\n";
        let result = ensure_yaml_line_breaks(input);

        assert_eq!(result, input, "改行が既にある場合はそのまま返すべき");
    }

    #[test]
    fn default_specification_template_parses_as_yaml_context() {
        let editor = SpecificationEditor::default();

        assert_eq!(editor.lines, vec![String::new()]);
        assert_eq!(editor.cursor_row, 0);
        assert_eq!(editor.cursor_col, 0);
        assert!(!editor.text().contains("system_name: \"\""));
    }

    #[test]
    fn ensure_yaml_line_breaks_repairs_missing_key_value_space() {
        assert_eq!(
            ensure_yaml_line_breaks("system_name:test"),
            "system_name: test"
        );
    }

    #[test]
    fn ensure_yaml_line_breaks_repairs_compact_list_items() {
        assert_eq!(ensure_yaml_line_breaks("-test"), "- test");
        assert_eq!(ensure_yaml_line_breaks("* test"), "- test");
        assert_eq!(ensure_yaml_line_breaks("goals:\n-test"), "goals:\n  - test");
        assert_eq!(
            ensure_yaml_line_breaks("constraints:\n-test"),
            "constraints:\n  - test"
        );
        assert_eq!(
            ensure_yaml_line_breaks("architecture:\n-test"),
            "architecture:\n  - test"
        );
        assert_eq!(ensure_yaml_line_breaks("rules:\n-test"), "rules:\n  - test");
    }

    #[test]
    fn submit_editor_records_raw_payload_dump() {
        render_trace::reset();
        let mut state = TuiState::new(empty_payload());
        state.editor_state.editor.clear();
        for ch in "system_name:test".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        let action = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SUPER));

        assert_eq!(action, TuiAction::Submit("system_name: test".to_string()));
        let trace = render_trace::snapshot();
        assert!(trace.contains(&"RAW_PAYLOAD_BEGIN"));
        assert!(trace.contains(&"RAW_PAYLOAD_END"));
        assert!(trace.contains(&"PAYLOAD_LINE_1 system_name: test"));
    }

    #[test]
    fn ensure_yaml_line_breaks_handles_empty_input() {
        assert_eq!(ensure_yaml_line_breaks(""), "");
    }

    #[test]
    fn ensure_yaml_line_breaks_handles_non_yaml_single_line() {
        let input = "fix parser bug";
        let result = ensure_yaml_line_breaks(input);
        assert_eq!(result, input, "YAML以外の入力は変更しない");
    }
}
