use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::nl::session::ConversationState;
use crate::service::dto::{ActionKind, IRState, SessionAppliedDiff};
use crate::session::sanitize_debug_leakage;
use crate::state::State;
use crate::tui::edit_block::render_edit_blocks;
use crate::tui::proc_strip::{ProcStripState, render_proc_strip};
use crate::tui::review_batch::ReviewBatchState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerFocus {
    Editor,
    SendButton,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposerUiMode {
    Idle,
    PatchReview,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmitEvent {
    pub input: String,
    pub eof_submit: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UiIntent {
    pub raw_input: String,
    pub system_hint: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DetailLine {
    pub raw_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UiDiffFile {
    pub file_path: String,
    pub unified_diff_excerpt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DiffView {
    pub title: String,
    pub summary: String,
    pub files: Vec<UiDiffFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PlanView {
    pub target: Option<String>,
    pub focus: Option<String>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultView {
    pub result: ExecutionResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UiPanels {
    pub plan: Option<PlanView>,
    pub diff: Option<DiffView>,
    pub result: Option<ResultView>,
    pub detail: Vec<DetailLine>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Idle,
    Editing,
    Submitted,
    Synced,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiMeta {
    pub last_action: ActionKind,
    pub status: Status,
}

impl Default for UiMeta {
    fn default() -> Self {
        Self {
            last_action: ActionKind::Analyze,
            status: Status::Idle,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UiState {
    pub intent: UiIntent,
    pub panels: UiPanels,
    pub meta: UiMeta,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub intent_raw: String,
    pub panels: UiPanels,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    UserTyped(String),
    UserSubmit,
    PlannerUpdated(PlanView),
    DiffUpdated(DiffView),
    ResultUpdated(ResultView),
    DetailAppended(DetailLine),
    SessionRestored(SessionSnapshot),
}

impl UiState {
    pub fn reduce(&mut self, ev: UiEvent) {
        match ev {
            UiEvent::UserTyped(s) => {
                self.intent.raw_input.push_str(&s);
                self.meta.status = Status::Editing;
            }
            UiEvent::UserSubmit => {
                self.intent.raw_input.clear();
                self.meta.status = Status::Submitted;
            }
            UiEvent::PlannerUpdated(p) => {
                self.panels.plan = Some(p);
                self.meta.last_action = ActionKind::Analyze;
                self.meta.status = Status::Synced;
            }
            UiEvent::DiffUpdated(d) => {
                self.panels.diff = Some(d);
                self.meta.last_action = ActionKind::Apply;
                self.meta.status = Status::Synced;
            }
            UiEvent::ResultUpdated(r) => {
                self.panels.result = Some(r);
                self.meta.status = Status::Synced;
            }
            UiEvent::DetailAppended(d) => {
                self.panels.detail.push(d);
            }
            UiEvent::SessionRestored(snap) => {
                self.intent.raw_input = snap.intent_raw;
                self.panels = snap.panels;
                self.meta.status = Status::Synced;
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UiPolicy {
    pub hide_debug: bool,
}

pub fn render_text(text: &str, policy: &UiPolicy) -> String {
    if policy.hide_debug {
        sanitize_debug_leakage(text)
    } else {
        text.to_string()
    }
}

pub fn render_intent(state: &UiState, policy: &UiPolicy) -> String {
    render_text(&state.intent.raw_input, policy)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComposerViewState {
    pub review: Option<ReviewBatchState>,
    pub ir_state: IRState,
    pub detail_expanded: bool,
    pub proc_strip: ProcStripState,
    pub state: State,
    pub mode: ComposerUiMode,
    pub focus: ComposerFocus,
    pub send_hovered: bool,
    pub branch: Option<String>,
    pub selected_snapshot: Option<String>,
    pub command_mode: bool,
    pub file_chips: Vec<String>,
    pub command_chips: Vec<String>,
    pub ui: UiState,
    intent_cursor: usize,
    send_button_rect: Option<Rect>,
}

impl ComposerViewState {
    pub fn new(transcript: Vec<String>, state: State) -> Self {
        let mut ui = UiState::default();
        for line in transcript {
            ui.reduce(UiEvent::DetailAppended(DetailLine { raw_text: line }));
        }
        Self {
            review: None,
            ir_state: IRState::default(),
            detail_expanded: false,
            proc_strip: ProcStripState::idle(),
            state,
            mode: ComposerUiMode::Idle,
            focus: ComposerFocus::Editor,
            send_hovered: false,
            branch: None,
            selected_snapshot: None,
            command_mode: false,
            file_chips: Vec::new(),
            command_chips: Vec::new(),
            ui,
            intent_cursor: 0,
            send_button_rect: None,
        }
    }

    pub fn sync_context(
        &mut self,
        conversation: &ConversationState,
        branch: Option<String>,
        selected_snapshot: Option<String>,
    ) {
        self.branch = branch;
        self.selected_snapshot = selected_snapshot;
        self.ir_state = conversation.ir_state.clone();
        self.ui.intent.system_hint = conversation.prompt_label().map(ToString::to_string);
        self.ui.reduce(UiEvent::PlannerUpdated(build_plan_view(
            conversation
                .last_target
                .as_ref()
                .map(|path| path.display().to_string()),
            conversation.prompt_label().map(ToString::to_string),
            conversation
                .ir_state
                .next_allowed_actions
                .iter()
                .map(|action| action.as_label().to_string())
                .collect(),
        )));
        if let Some(diff) = active_diff_ref(self).cloned() {
            self.ui.reduce(UiEvent::DiffUpdated(diff_view_from_session(
                "[DIFF] Latest Applied",
                &diff,
            )));
        } else {
            self.ui.panels.diff = None;
        }
        self.sync_buffer_metadata();
    }

    pub fn push_transcript_line(&mut self, line: impl Into<String>) {
        let raw_text = line.into();
        if raw_text.is_empty() {
            return;
        }
        self.ui
            .reduce(UiEvent::DetailAppended(DetailLine { raw_text }));
        self.restore_intent_document_focus();
    }

    pub fn activate_review(&mut self, review: ReviewBatchState) {
        self.review = Some(review);
        self.mode = ComposerUiMode::PatchReview;
        self.restore_intent_document_focus();
    }

    pub fn set_execution_result(&mut self, result: ExecutionResult) {
        self.ui
            .reduce(UiEvent::ResultUpdated(ResultView { result }));
    }

    pub fn reset_review_session(&mut self) {
        self.review = None;
        self.mode = ComposerUiMode::Idle;
        self.send_hovered = false;
        self.restore_intent_document_focus();
    }

    pub fn sync_buffer_metadata(&mut self) {
        let text = self.ui.intent.raw_input.clone();
        let trimmed = text.trim_start();
        self.command_mode = trimmed.starts_with('/');
        self.command_chips = if self.command_mode {
            trimmed
                .split_whitespace()
                .next()
                .map(|token| vec![token.to_string()])
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        self.file_chips = extract_file_reference_chips(&text);
    }

    pub fn restore_intent_document_focus(&mut self) {
        self.focus = ComposerFocus::Editor;
        self.send_hovered = false;
        self.intent_cursor = self.ui.intent.raw_input.chars().count();
        self.ensure_cursor_visible();
    }

    pub fn ensure_cursor_visible(&mut self) {}

    pub fn intent_text(&self) -> String {
        self.ui.intent.raw_input.clone()
    }

    pub fn intent_is_blank(&self) -> bool {
        self.ui.intent.raw_input.trim().is_empty()
    }

    pub fn intent_cursor(&self) -> (usize, usize) {
        (0, self.intent_cursor)
    }

    pub fn insert_intent_text(&mut self, value: &str) {
        self.ui.reduce(UiEvent::UserTyped(value.to_string()));
        self.intent_cursor = self.ui.intent.raw_input.chars().count();
        self.sync_buffer_metadata();
    }

    pub fn backspace_intent(&mut self) {
        self.ui.intent.raw_input.pop();
        self.intent_cursor = self.ui.intent.raw_input.chars().count();
        self.sync_buffer_metadata();
    }

    pub fn restore_session_snapshot(&mut self, snapshot: SessionSnapshot) {
        self.ui.reduce(UiEvent::SessionRestored(snapshot));
        self.intent_cursor = self.ui.intent.raw_input.chars().count();
        self.sync_buffer_metadata();
    }

    pub fn detail_lines(&self) -> Vec<String> {
        self.ui
            .panels
            .detail
            .iter()
            .map(|line| line.raw_text.clone())
            .collect()
    }

    pub fn detail_len(&self) -> usize {
        self.ui.panels.detail.len()
    }

    fn take_submit_event(&mut self, eof_submit: bool) -> Option<SubmitEvent> {
        let input = self.intent_text();
        if input.trim().is_empty() {
            return None;
        }
        self.ui.reduce(UiEvent::UserSubmit);
        self.intent_cursor = 0;
        Some(SubmitEvent { input, eof_submit })
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> ComposerAction {
        if key.kind != KeyEventKind::Press {
            return ComposerAction::None;
        }
        match self.focus {
            ComposerFocus::Editor => self.handle_editor_key(key),
            ComposerFocus::SendButton => self.handle_send_button_key(key),
        }
    }

    pub fn handle_mouse_event(&mut self, mouse: MouseEvent) -> ComposerAction {
        let hovered = self
            .send_button_rect
            .map(|rect| point_in_rect(mouse.column, mouse.row, rect))
            .unwrap_or(false);
        self.send_hovered = hovered;
        match mouse.kind {
            MouseEventKind::Down(_) if hovered => {
                self.focus = ComposerFocus::SendButton;
                let action = self
                    .take_submit_event(false)
                    .map(ComposerAction::Submit)
                    .unwrap_or(ComposerAction::None);
                self.restore_intent_document_focus();
                action
            }
            MouseEventKind::Moved => ComposerAction::None,
            _ => ComposerAction::None,
        }
    }

    fn handle_editor_key(&mut self, key: KeyEvent) -> ComposerAction {
        match key.code {
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.detail_expanded = !self.detail_expanded;
                ComposerAction::None
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => ComposerAction::None,
            KeyCode::Enter => self
                .take_submit_event(false)
                .map(ComposerAction::Submit)
                .unwrap_or(ComposerAction::None),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => self
                .take_submit_event(true)
                .map(ComposerAction::Submit)
                .unwrap_or(ComposerAction::Exit),
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Esc => {
                self.restore_intent_document_focus();
                ComposerAction::None
            }
            KeyCode::Backspace => {
                self.backspace_intent();
                self.sync_buffer_metadata();
                ComposerAction::None
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.ui.reduce(UiEvent::UserTyped(ch.to_string()));
                self.intent_cursor = self.ui.intent.raw_input.chars().count();
                self.sync_buffer_metadata();
                ComposerAction::None
            }
            _ => ComposerAction::None,
        }
    }

    fn handle_send_button_key(&mut self, key: KeyEvent) -> ComposerAction {
        match key.code {
            KeyCode::Char('l') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.detail_expanded = !self.detail_expanded;
                ComposerAction::None
            }
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Esc => {
                self.restore_intent_document_focus();
                ComposerAction::None
            }
            KeyCode::Enter => self
                .take_submit_event(false)
                .map(ComposerAction::Submit)
                .unwrap_or(ComposerAction::None),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => self
                .take_submit_event(true)
                .map(ComposerAction::Submit)
                .unwrap_or(ComposerAction::Exit),
            _ => ComposerAction::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposerAction {
    None,
    Submit(SubmitEvent),
    Exit,
    ForceQuit,
}

pub fn render_composer(frame: &mut Frame, state: &mut ComposerViewState) {
    let area = frame.area();
    frame.render_widget(Clear, area);

    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(review_height(state)),
            Constraint::Length(plan_height(state)),
            Constraint::Length(diff_height(state)),
            Constraint::Length(result_height()),
            Constraint::Length(detail_height(state)),
            Constraint::Length(8),
            Constraint::Length(2),
        ])
        .split(area);

    render_context_bar(frame, state, vertical[0]);
    render_proc_strip(frame, &state.proc_strip, vertical[1]);
    if let Some(review) = &state.review {
        render_edit_blocks(frame, review, vertical[2]);
    }
    render_plan_panel(frame, state, vertical[3]);
    render_diff_panel(frame, state, vertical[4]);
    render_result_panel(frame, state, vertical[5]);
    render_detail_panel(frame, state, vertical[6]);
    render_composer_panel(frame, state, vertical[7]);
    render_footer_hints(frame, state, vertical[8]);
}

fn ui_policy() -> UiPolicy {
    UiPolicy { hide_debug: true }
}

fn build_plan_view(
    target: Option<String>,
    focus: Option<String>,
    next_actions: Vec<String>,
) -> PlanView {
    PlanView {
        target,
        focus,
        next_actions,
    }
}

fn diff_view_from_session(title: &str, diff: &SessionAppliedDiff) -> DiffView {
    DiffView {
        title: title.to_string(),
        summary: diff.summary.clone(),
        files: diff
            .files
            .iter()
            .map(|file| UiDiffFile {
                file_path: file.file_path.clone(),
                unified_diff_excerpt: file.unified_diff_excerpt.clone(),
            })
            .collect(),
    }
}

fn review_height(state: &ComposerViewState) -> u16 {
    state
        .review
        .as_ref()
        .map(|review| {
            review
                .groups
                .iter()
                .map(|group| 3 + (group.block_indices.len() as u16 * 10))
                .sum::<u16>()
                .min(24)
        })
        .unwrap_or(0)
}

fn plan_height(state: &ComposerViewState) -> u16 {
    let body = plan_lines(state).len().max(1) as u16;
    (body + 2).clamp(3, 6)
}

fn diff_height(state: &ComposerViewState) -> u16 {
    let body_lines = diff_panel_lines(state).len().max(1) as u16;
    (body_lines + 2).clamp(8, 22)
}

fn result_height() -> u16 {
    6
}

fn detail_height(state: &ComposerViewState) -> u16 {
    if state.detail_expanded { 10 } else { 4 }
}

fn render_context_bar(frame: &mut Frame, state: &ComposerViewState, area: Rect) {
    let target = state
        .ui
        .panels
        .plan
        .as_ref()
        .and_then(|plan| plan.target.clone());
    let chips = [
        state
            .branch
            .as_ref()
            .map(|branch| format!("branch: {branch}")),
        target.as_ref().map(|target| format!("target: {target}")),
        state
            .selected_snapshot
            .as_ref()
            .map(|snapshot| format!("snapshot: {snapshot}")),
        state.command_mode.then_some("mode: /command".to_string()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .title(" Context ")
        .border_style(Style::default().fg(Color::Cyan));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    frame.render_widget(
        Paragraph::new(Line::from(
            chips
                .into_iter()
                .map(|chip| render_chip_span(&chip))
                .collect::<Vec<_>>(),
        )),
        inner,
    );
}

fn render_plan_panel(frame: &mut Frame, state: &ComposerViewState, area: Rect) {
    let lines = plan_lines(state)
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    render_section_panel(frame, area, lines, "[PLAN]", Color::Blue);
}

fn render_diff_panel(frame: &mut Frame, state: &ComposerViewState, area: Rect) {
    let content = diff_panel_lines(state)
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    render_section_panel(frame, area, content, "[DIFF]", Color::Cyan);
}

fn render_result_panel(frame: &mut Frame, state: &ComposerViewState, area: Rect) {
    let content = format_execution_result(execution_result_for_view(state))
        .lines()
        .map(|line| Line::from(line.to_string()))
        .collect::<Vec<_>>();
    render_section_panel(frame, area, content, "[RESULT]", Color::Yellow);
}

fn render_detail_panel(frame: &mut Frame, state: &ComposerViewState, area: Rect) {
    let content = if state.detail_expanded {
        detail_lines_expanded(state)
    } else {
        detail_lines_collapsed(state)
    }
    .into_iter()
    .map(Line::from)
    .collect::<Vec<_>>();
    render_section_panel(frame, area, content, "[DETAIL]", Color::DarkGray);
}

fn diff_panel_lines(state: &ComposerViewState) -> Vec<String> {
    let policy = ui_policy();
    let mut content = Vec::new();
    content.push(render_text("================================", &policy));
    if let Some(diff) = state.ui.panels.diff.as_ref() {
        content.push(render_text(&diff.title, &policy));
        content.push(render_text(&diff.summary, &policy));
        content.push(render_text("================================", &policy));
        for file in &diff.files {
            content.push(render_text(&format!("File: {}", file.file_path), &policy));
            content.push(render_text("--------------------------------", &policy));
            for line in file.unified_diff_excerpt.lines() {
                let sanitized = render_text(line, &policy);
                if !sanitized.is_empty() {
                    content.push(sanitized);
                }
            }
            content.push(String::new());
        }
    } else {
        content.push(render_text("No changes detected.", &policy));
        content.push(render_text(
            "Run `apply` or `refactor` to generate changes.",
            &policy,
        ));
        content.push(render_text("================================", &policy));
    }
    content
}

fn render_section_panel(
    frame: &mut Frame,
    area: Rect,
    content: Vec<Line<'static>>,
    title: &str,
    color: Color,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .title(format!(" {title} "))
        .border_style(Style::default().fg(color));
    frame.render_widget(
        Paragraph::new(content)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn plan_lines(state: &ComposerViewState) -> Vec<String> {
    let policy = ui_policy();
    let mut lines = Vec::new();
    let Some(plan) = state.ui.panels.plan.as_ref() else {
        return vec![render_text("No active plan.", &policy)];
    };
    if let Some(target) = &plan.target {
        lines.push(render_text(&format!("Target: {target}"), &policy));
    }
    if let Some(header) = &plan.focus {
        lines.push(render_text(&format!("Focus: {header}"), &policy));
    }
    if plan.next_actions.is_empty() {
        if lines.is_empty() {
            lines.push(render_text("No active plan.", &policy));
        }
        return lines;
    }
    lines.push(render_text(
        &format!("Next: {}", plan.next_actions.join(" / ")),
        &policy,
    ));
    lines
}

fn active_diff_ref(state: &ComposerViewState) -> Option<&SessionAppliedDiff> {
    state
        .ir_state
        .active_transaction
        .as_ref()
        .and_then(|tx| tx.latest_diff_ref.as_ref())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionResult {
    Success {
        files_changed: usize,
        lines_added: usize,
        lines_removed: usize,
    },
    Failure {
        reason: String,
    },
    NoOp,
    RolledBack {
        reason: String,
    },
}

pub fn format_execution_result(result: ExecutionResult) -> String {
    match result {
        ExecutionResult::Success {
            files_changed,
            lines_added,
            lines_removed,
        } => format!(
            "Refactoring applied successfully.\n{files_changed} files changed, +{lines_added} -{lines_removed} lines."
        ),
        ExecutionResult::Failure { reason } => {
            let trimmed = reason.trim();
            if trimmed.is_empty() || trimmed == "validation_failed" {
                "Validation failed. Changes were not applied.".to_string()
            } else {
                format!("Execution failed: {trimmed}")
            }
        }
        ExecutionResult::NoOp => "No changes detected.".to_string(),
        ExecutionResult::RolledBack { reason } => {
            let trimmed = reason.trim();
            if trimmed == "validation_failed" {
                "Rolled back due to validation failure.".to_string()
            } else {
                "Changes were rolled back.".to_string()
            }
        }
    }
}

fn execution_result_for_view(state: &ComposerViewState) -> ExecutionResult {
    state
        .ui
        .panels
        .result
        .as_ref()
        .map(|result| result.result.clone())
        .or_else(|| active_diff_ref(state).map(success_result_from_diff))
        .or_else(|| {
            state
                .review
                .as_ref()
                .and_then(|review| review.preview_diff_snapshot())
                .map(|diff| success_result_from_diff(&diff))
        })
        .unwrap_or(ExecutionResult::NoOp)
}

fn success_result_from_diff(diff: &SessionAppliedDiff) -> ExecutionResult {
    ExecutionResult::Success {
        files_changed: diff.files_changed,
        lines_added: diff.lines_added,
        lines_removed: diff.lines_removed,
    }
}

fn detail_lines_collapsed(state: &ComposerViewState) -> Vec<String> {
    let policy = ui_policy();
    let count = state.ui.panels.detail.len();
    let latest = state
        .ui
        .panels
        .detail
        .last()
        .map(|line| render_text(&line.raw_text, &policy))
        .unwrap_or_default();
    let summary = if count == 0 {
        render_text("No detail logs.", &policy)
    } else {
        render_text(&format!("{count} detail line(s) available."), &policy)
    };
    let preview = if latest.is_empty() {
        render_text("Press Ctrl+L to expand detail.", &policy)
    } else {
        render_text(
            &format!("Latest: {}", latest.lines().next().unwrap_or_default()),
            &policy,
        )
    };
    vec![
        summary,
        preview,
        render_text("Press Ctrl+L to expand detail.", &policy),
    ]
}

fn detail_lines_expanded(state: &ComposerViewState) -> Vec<String> {
    let policy = ui_policy();
    if state.ui.panels.detail.is_empty() {
        return vec![
            render_text("No detail logs.", &policy),
            render_text("Press Ctrl+L to collapse detail.", &policy),
        ];
    }
    let mut lines = state
        .ui
        .panels
        .detail
        .iter()
        .rev()
        .take(6)
        .rev()
        .map(|line| render_text(&line.raw_text, &policy))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>();
    lines.push(render_text("Press Ctrl+L to collapse detail.", &policy));
    lines
}

fn render_composer_panel(frame: &mut Frame, state: &mut ComposerViewState, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .split(area);
    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(10), Constraint::Length(14)])
        .split(rows[1]);

    let focus_style = if state.focus == ComposerFocus::Editor {
        Style::default()
            .fg(Color::Cyan)
            .bg(Color::Black)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let chip_spans = state
        .file_chips
        .iter()
        .map(|chip| render_chip_span(chip))
        .chain(
            state
                .command_chips
                .iter()
                .map(|chip| render_chip_span(chip)),
        )
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(Line::from(chip_spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_set(border::ROUNDED)
                .title(" Composer ")
                .border_style(focus_style),
        ),
        rows[0],
    );

    let editor_block = Block::default()
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .title(" Intent Document ")
        .border_style(focus_style);
    let editor_inner = editor_block.inner(body[0]);
    frame.render_widget(
        Paragraph::new(if state.intent_is_blank() {
            render_text(
                "> Type a command (analyze / refactor / apply)",
                &ui_policy(),
            )
        } else {
            render_intent(&state.ui, &ui_policy())
        })
        .style(if state.intent_is_blank() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        })
        .block(editor_block)
        .wrap(Wrap { trim: false }),
        body[0],
    );

    let send_style = if state.send_hovered || state.focus == ComposerFocus::SendButton {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::White).bg(Color::Blue)
    };
    let send_block = Block::default()
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .border_style(send_style)
        .title(" Send ");
    let send_inner = send_block.inner(body[1]);
    state.send_button_rect = Some(body[1]);
    frame.render_widget(send_block, body[1]);
    frame.render_widget(
        Paragraph::new("Enter")
            .style(send_style)
            .alignment(Alignment::Center),
        send_inner,
    );

    let hint = if state.focus == ComposerFocus::Editor {
        "Single-line input, cursor follows the tail"
    } else {
        "Tab/Esc returns to editor"
    };
    frame.render_widget(
        Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)),
        rows[2],
    );

    if state.focus == ComposerFocus::Editor {
        let (row, col) = state.intent_cursor();
        frame.set_cursor_position((editor_inner.x + col as u16, editor_inner.y + row as u16));
    }
}

fn render_footer_hints(frame: &mut Frame, state: &ComposerViewState, area: Rect) {
    let mut segments = vec![
        Span::styled("@ file", Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled("/ command", Style::default().fg(Color::Cyan)),
        Span::raw("  "),
        Span::styled("Enter send", Style::default().fg(Color::Green)),
        Span::raw("  "),
        Span::styled("Ctrl+L detail", Style::default().fg(Color::Blue)),
    ];
    if state.review.is_some() {
        segments.extend([
            Span::raw("  "),
            Span::styled("Space select", Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::styled("A apply", Style::default().fg(Color::Green)),
            Span::raw("  "),
            Span::styled("D discard", Style::default().fg(Color::Red)),
            Span::raw("  "),
            Span::styled("R rollback", Style::default().fg(Color::Magenta)),
            Span::raw("  "),
            Span::styled("[/] groups", Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::styled("E/C expand", Style::default().fg(Color::Cyan)),
            Span::raw("  "),
            Span::styled("J/K focus", Style::default().fg(Color::Yellow)),
        ]);
    }
    let hints = Line::from(segments);
    frame.render_widget(
        Paragraph::new(hints)
            .block(Block::default().borders(Borders::TOP))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_chip_span(label: &str) -> Span<'static> {
    Span::styled(
        format!(" {label} "),
        Style::default()
            .fg(Color::White)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::BOLD),
    )
}

fn extract_file_reference_chips(text: &str) -> Vec<String> {
    let mut chips = text
        .split_whitespace()
        .filter_map(|token| {
            let trimmed = token.trim_matches(|ch: char| matches!(ch, ',' | '。' | ':' | ';'));
            if trimmed.contains('*') {
                return None;
            }
            if let Some(rest) = trimmed.strip_prefix('@') {
                return (!rest.is_empty()).then(|| format!("@{rest}"));
            }
            let looks_like_path = (trimmed.contains('/') || trimmed.ends_with(".rs"))
                && !trimmed.starts_with('/')
                && Path::new(trimmed)
                    .components()
                    .all(|component| !component.as_os_str().is_empty());
            looks_like_path.then(|| trimmed.to_string())
        })
        .collect::<Vec<_>>();
    chips.sort();
    chips.dedup();
    chips
}

fn point_in_rect(column: u16, row: u16, rect: Rect) -> bool {
    column >= rect.x && column < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::nl::session::ConversationState;
    use crate::service::dto::{IRActiveTransaction, SessionAppliedFileDiff};

    use super::*;

    #[test]
    fn composer_buffer_appends_at_tail_and_keeps_cursor_in_sync() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.insert_intent_text("abc");
        assert_eq!(state.intent_text(), "abc");
        assert_eq!(state.intent_cursor(), (0, 3));
    }

    #[test]
    fn wildcard_token_in_prose_does_not_mutate_prompt_state() {
        let mut conversation = ConversationState {
            last_target: Some(PathBuf::from("apps/cli/src/coding.rs")),
            ..ConversationState::default()
        };
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.sync_context(&conversation, Some("main".to_string()), None);
        let hint_before = state.ui.intent.system_hint.clone();
        let target_before = state
            .ui
            .panels
            .plan
            .as_ref()
            .and_then(|plan| plan.target.clone());

        state.insert_intent_text("ImportRebinding-only の diff では *_interface.rs を生成しない。");

        assert_eq!(state.ui.intent.system_hint, hint_before);
        assert_eq!(
            state
                .ui
                .panels
                .plan
                .as_ref()
                .and_then(|plan| plan.target.clone()),
            target_before
        );
        assert!(state.file_chips.is_empty(), "{:?}", state.file_chips);

        conversation.last_target = Some(PathBuf::from("apps/cli/src/coding.rs"));
        state.sync_context(&conversation, Some("main".to_string()), None);
        assert_eq!(
            state
                .ui
                .panels
                .plan
                .as_ref()
                .and_then(|plan| plan.target.clone()),
            target_before
        );
    }

    #[test]
    fn single_line_spec_executes_only_on_submit() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        let mut executions = Vec::new();

        state.insert_intent_text(
            "Semantic Interface Extraction Guard を追加する。 対象は apps/cli/src/coding.rs。",
        );
        assert!(executions.is_empty());
        assert!(executions.is_empty());

        let submit = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(executions.is_empty());
        let ComposerAction::Submit(event) = submit else {
            panic!("expected submit");
        };
        executions.push(event.input);

        assert_eq!(executions.len(), 1);
        assert_eq!(
            executions[0],
            "Semantic Interface Extraction Guard を追加する。 対象は apps/cli/src/coding.rs。"
        );
        assert!(state.intent_is_blank());
    }

    #[test]
    fn typing_updates_mode_metadata_without_dispatch() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.focus = ComposerFocus::SendButton;
        let transcript_before = state.detail_lines();

        for ch in ['@', 'f', 'i', 'l', 'e'] {
            assert_eq!(
                state.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
                ComposerAction::None
            );
        }

        assert_eq!(state.detail_lines(), transcript_before);
        assert_eq!(state.focus, ComposerFocus::SendButton);
        assert!(state.intent_is_blank());
        assert!(
            !state.command_mode,
            "typing file mention text must not trigger command dispatch state"
        );

        state.focus = ComposerFocus::Editor;
        for ch in "/analyze .".chars() {
            assert_eq!(
                state.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
                ComposerAction::None
            );
        }

        assert!(state.command_mode);
        assert!(state.file_chips.is_empty());
    }

    #[test]
    fn transcript_append_restores_editor_focus() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.focus = ComposerFocus::SendButton;
        state.push_transcript_line("long transcript dump");

        assert_eq!(state.focus, ComposerFocus::Editor);
        assert_eq!(state.intent_cursor(), (0, 0));
    }

    #[test]
    fn transcript_append_preserves_raw_detail_state() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.push_transcript_line("visible\nTRACE:R1:ENTER\nDEBUG:hook\nstill visible");
        assert_eq!(
            state.detail_lines(),
            vec!["visible\nTRACE:R1:ENTER\nDEBUG:hook\nstill visible".to_string()]
        );
    }

    #[test]
    fn submit_document_preserves_raw_intent_state() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.insert_intent_text("hello\nTRACE:R1\nworld");
        let submitted = state.take_submit_event(false).expect("submit");
        assert_eq!(submitted.input, "hello\nTRACE:R1\nworld");
    }

    #[test]
    fn detail_lines_hide_debug_leakage() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.ui.panels.detail = vec![DetailLine {
            raw_text: "hello\nTRACE:R1\nworld".to_string(),
        }];
        let lines = detail_lines_expanded(&state);
        assert!(lines.iter().any(|line| line == "hello\nworld"));
        assert!(!lines.iter().any(|line| line.contains("TRACE:")));
    }

    #[test]
    fn render_intent_hides_debug_leakage() {
        let mut state = UiState::default();
        state.reduce(UiEvent::UserTyped(
            "visible\nTRACE:R2\nDEBUG:x\nstill visible".to_string(),
        ));
        assert_eq!(
            render_intent(&state, &UiPolicy { hide_debug: true }),
            "visible\nstill visible"
        );
    }

    #[test]
    fn buffer_never_contains_trace() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.insert_intent_text("TRACE:R1");
        assert!(state.intent_text().contains("TRACE:"));
        assert!(!render_intent(&state.ui, &UiPolicy { hide_debug: true }).contains("TRACE:"));
    }

    #[test]
    fn session_restore_preserves_raw_system_reinjection_until_render() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.restore_session_snapshot(SessionSnapshot {
            intent_raw: "TRACE:R1 something".to_string(),
            panels: UiPanels::default(),
        });
        assert_eq!(state.intent_text(), "TRACE:R1 something");
        assert_eq!(render_intent(&state.ui, &UiPolicy { hide_debug: true }), "");
    }

    #[test]
    fn render_text_removes_embedded_trace_tokens() {
        assert_eq!(
            render_text("hello TRACE:R1 world", &UiPolicy { hide_debug: true }),
            "hello TRACE:R1 world"
        );
    }

    #[test]
    fn render_text_normalizes_newlines_and_drops_empty_debug_lines() {
        assert_eq!(
            render_text("TRACE:R1\r\nTRACE:R2", &UiPolicy { hide_debug: true }),
            ""
        );
    }

    #[test]
    fn tab_shift_tab_and_esc_never_leave_editor_deadlocked() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.insert_intent_text("/command");

        assert_eq!(
            state.handle_key_event(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            ComposerAction::None
        );
        assert_eq!(state.focus, ComposerFocus::Editor);

        state.focus = ComposerFocus::SendButton;
        assert_eq!(
            state.handle_key_event(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT)),
            ComposerAction::None
        );
        assert_eq!(state.focus, ComposerFocus::Editor);

        state.focus = ComposerFocus::SendButton;
        assert_eq!(
            state.handle_key_event(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            ComposerAction::None
        );
        assert_eq!(state.focus, ComposerFocus::Editor);
    }

    #[test]
    fn enter_dispatches_exactly_once_after_typing() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        let transcript_before = state.detail_lines();
        for ch in "analyze src/".chars() {
            assert_eq!(
                state.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
                ComposerAction::None
            );
        }

        assert_eq!(state.detail_lines(), transcript_before);

        let first = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let ComposerAction::Submit(event) = first else {
            panic!("expected submit on enter");
        };
        assert_eq!(event.input, "analyze src/");
        assert!(state.intent_is_blank());

        let second = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(second, ComposerAction::None);
    }

    #[test]
    fn enter_on_blank_buffer_is_no_op() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        assert_eq!(
            state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            ComposerAction::None
        );
        assert!(state.intent_is_blank());
    }

    #[test]
    fn backspace_removes_from_tail_only() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.insert_intent_text("abc");

        assert_eq!(
            state.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
            ComposerAction::None
        );
        assert_eq!(state.intent_text(), "ab");
        assert_eq!(state.intent_cursor(), (0, 2));
    }

    #[test]
    fn cursor_invariant_holds_after_supported_operations() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        for ch in "hello".chars() {
            assert_eq!(
                state.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
                ComposerAction::None
            );
            assert_eq!(state.intent_cursor().1, state.intent_text().chars().count());
        }

        assert_eq!(
            state.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
            ComposerAction::None
        );
        assert_eq!(state.intent_cursor().1, state.intent_text().chars().count());

        let action = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(action, ComposerAction::Submit(_)));
        assert_eq!(state.intent_cursor().1, state.intent_text().chars().count());
    }

    #[test]
    fn plan_uses_ir_next_actions_only() {
        let mut state = ComposerViewState::new(vec!["existing".to_string()], State::Idle);
        state.ui.panels.plan = Some(build_plan_view(
            None,
            None,
            vec!["apply".to_string(), "validate".to_string()],
        ));

        let lines = plan_lines(&state);

        assert_eq!(lines, vec!["Next: apply / validate".to_string()]);
    }

    #[test]
    fn session_diff_reads_ir_transaction_only() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.ir_state.active_transaction = Some(IRActiveTransaction {
            transaction_id: "tx:apps/cli/src/repl.rs".to_string(),
            canonical_target: PathBuf::from("apps/cli/src/repl.rs"),
            pending: false,
            applied: true,
            validated: false,
            rollback_available: true,
            latest_diff_ref: Some(SessionAppliedDiff {
                summary: "latest applied change (1 file)".to_string(),
                files: vec![SessionAppliedFileDiff {
                    file_path: "apps/cli/src/repl.rs".to_string(),
                    unified_diff_excerpt: "+ updated".to_string(),
                }],
                files_changed: 1,
                lines_added: 1,
                lines_removed: 0,
            }),
            latest_build_ok: None,
            file_hash: None,
        });

        state.ui.panels.diff = Some(diff_view_from_session(
            "[DIFF] Latest Applied",
            state
                .ir_state
                .active_transaction
                .as_ref()
                .and_then(|tx| tx.latest_diff_ref.as_ref())
                .expect("diff"),
        ));

        assert_eq!(diff_height(&state), 10);
        assert_eq!(
            active_diff_ref(&state).map(|diff| diff.summary.as_str()),
            Some("latest applied change (1 file)")
        );
    }

    #[test]
    fn formatter_renders_success_in_two_lines() {
        let rendered = format_execution_result(ExecutionResult::Success {
            files_changed: 2,
            lines_added: 12,
            lines_removed: 5,
        });
        assert_eq!(
            rendered,
            "Refactoring applied successfully.\n2 files changed, +12 -5 lines."
        );
    }

    #[test]
    fn formatter_renders_noop_and_rollback_messages() {
        assert_eq!(
            format_execution_result(ExecutionResult::NoOp),
            "No changes detected."
        );
        assert_eq!(
            format_execution_result(ExecutionResult::RolledBack {
                reason: "validation_failed".to_string(),
            }),
            "Rolled back due to validation failure."
        );
    }

    #[test]
    fn execution_result_defaults_from_active_diff_counts() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.ir_state.active_transaction = Some(IRActiveTransaction {
            transaction_id: "tx:apps/cli/src/repl.rs".to_string(),
            canonical_target: PathBuf::from("apps/cli/src/repl.rs"),
            pending: false,
            applied: true,
            validated: false,
            rollback_available: true,
            file_hash: None,
            latest_diff_ref: Some(SessionAppliedDiff {
                summary: "1 files changed, +12 -5 lines".to_string(),
                files: vec![SessionAppliedFileDiff {
                    file_path: "apps/cli/src/repl.rs".to_string(),
                    unified_diff_excerpt: "+ updated".to_string(),
                }],
                files_changed: 1,
                lines_added: 12,
                lines_removed: 5,
            }),
            latest_build_ok: None,
        });

        assert_eq!(
            execution_result_for_view(&state),
            ExecutionResult::Success {
                files_changed: 1,
                lines_added: 12,
                lines_removed: 5,
            }
        );
    }
}
