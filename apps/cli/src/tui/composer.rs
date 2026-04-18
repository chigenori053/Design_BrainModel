use std::path::Path;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::border,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

use crate::nl::session::ConversationState;
use crate::service::dto::{IRState, SessionAppliedDiff};
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
pub struct ComposerBuffer {
    content: String,
    cursor: usize,
}

impl ComposerBuffer {
    pub fn new() -> Self {
        Self {
            content: String::new(),
            cursor: 0,
        }
    }

    pub fn text(&self) -> String {
        self.content.clone()
    }

    pub fn is_blank(&self) -> bool {
        self.content.trim().is_empty()
    }

    pub fn cursor(&self) -> (usize, usize) {
        (0, self.cursor)
    }

    pub fn move_cursor_to_end(&mut self) {
        self.cursor = self.content.chars().count();
        self.assert_invariant();
    }

    pub fn insert_char(&mut self, ch: char) {
        self.content.push(ch);
        self.cursor = self.content.chars().count();
        self.assert_invariant();
    }

    pub fn insert_str(&mut self, value: &str) {
        for ch in value.chars() {
            self.insert_char(ch);
        }
    }

    pub fn backspace(&mut self) {
        if !self.content.is_empty() {
            self.content.pop();
        }
        self.cursor = self.content.chars().count();
        self.assert_invariant();
    }

    pub fn submit_document(&mut self, eof_submit: bool) -> Option<SubmitEvent> {
        let input = self.text();
        if input.trim().is_empty() {
            return None;
        }
        *self = Self::new();
        Some(SubmitEvent { input, eof_submit })
    }

    fn assert_invariant(&self) {
        debug_assert_eq!(self.cursor, self.content.chars().count());
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComposerViewState {
    pub transcript: Vec<String>,
    pub review: Option<ReviewBatchState>,
    pub ir_state: IRState,
    pub execution_result: Option<ExecutionResult>,
    pub detail_expanded: bool,
    pub proc_strip: ProcStripState,
    pub state: State,
    pub mode: ComposerUiMode,
    pub focus: ComposerFocus,
    pub send_hovered: bool,
    pub branch: Option<String>,
    pub current_target: Option<String>,
    pub selected_snapshot: Option<String>,
    pub command_mode: bool,
    pub file_chips: Vec<String>,
    pub command_chips: Vec<String>,
    pub buffer: ComposerBuffer,
    pub prompt_header: Option<String>,
    send_button_rect: Option<Rect>,
}

impl ComposerViewState {
    pub fn new(transcript: Vec<String>, state: State) -> Self {
        Self {
            transcript,
            review: None,
            ir_state: IRState::default(),
            execution_result: None,
            detail_expanded: false,
            proc_strip: ProcStripState::idle(),
            state,
            mode: ComposerUiMode::Idle,
            focus: ComposerFocus::Editor,
            send_hovered: false,
            branch: None,
            current_target: None,
            selected_snapshot: None,
            command_mode: false,
            file_chips: Vec::new(),
            command_chips: Vec::new(),
            buffer: ComposerBuffer::new(),
            prompt_header: None,
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
        self.current_target = conversation
            .last_target
            .as_ref()
            .map(|path| path.display().to_string());
        self.selected_snapshot = selected_snapshot;
        self.prompt_header = conversation.prompt_label().map(ToString::to_string);
        self.ir_state = conversation.ir_state.clone();
        self.sync_buffer_metadata();
    }

    pub fn push_transcript_line(&mut self, line: impl Into<String>) {
        let sanitized = sanitize_intent_document(&line.into());
        if sanitized.is_empty() {
            return;
        }
        self.transcript.push(sanitized);
        self.restore_intent_document_focus();
    }

    pub fn activate_review(&mut self, review: ReviewBatchState) {
        self.review = Some(review);
        self.mode = ComposerUiMode::PatchReview;
        self.restore_intent_document_focus();
    }

    pub fn set_execution_result(&mut self, result: ExecutionResult) {
        self.execution_result = Some(result);
    }

    pub fn reset_review_session(&mut self) {
        self.review = None;
        self.mode = ComposerUiMode::Idle;
        self.send_hovered = false;
        self.restore_intent_document_focus();
    }

    pub fn sync_buffer_metadata(&mut self) {
        let text = self.buffer.text();
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
        self.buffer.move_cursor_to_end();
        self.ensure_cursor_visible();
    }

    pub fn ensure_cursor_visible(&mut self) {}

    pub fn handle_key_event(&mut self, key: KeyEvent) -> ComposerAction {
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
                    .buffer
                    .submit_document(false)
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
                .buffer
                .submit_document(false)
                .map(ComposerAction::Submit)
                .unwrap_or(ComposerAction::None),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => self
                .buffer
                .submit_document(true)
                .map(ComposerAction::Submit)
                .unwrap_or(ComposerAction::Exit),
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Esc => {
                self.restore_intent_document_focus();
                ComposerAction::None
            }
            KeyCode::Backspace => {
                self.buffer.backspace();
                self.sync_buffer_metadata();
                ComposerAction::None
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.buffer.insert_char(ch);
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
                .buffer
                .submit_document(false)
                .map(ComposerAction::Submit)
                .unwrap_or(ComposerAction::None),
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => self
                .buffer
                .submit_document(true)
                .map(ComposerAction::Submit)
                .unwrap_or(ComposerAction::Exit),
            _ => ComposerAction::None,
        }
    }
}

fn sanitize_intent_document(input: &str) -> String {
    sanitize_debug_leakage(input)
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
    let chips = [
        state
            .branch
            .as_ref()
            .map(|branch| format!("branch: {branch}")),
        state
            .current_target
            .as_ref()
            .map(|target| format!("target: {target}")),
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
    let mut content = Vec::new();
    content.push("================================".to_string());
    if let Some((title, diff)) = active_diff_panel(state) {
        content.push(title);
        content.push(diff.summary.clone());
        content.push("================================".to_string());
        for file in &diff.files {
            content.push(format!("File: {}", file.file_path));
            content.push("--------------------------------".to_string());
            for line in file.unified_diff_excerpt.lines() {
                content.push(line.to_string());
            }
            content.push(String::new());
        }
    } else {
        content.push("No changes detected.".to_string());
        content.push("Run `apply` or `refactor` to generate changes.".to_string());
        content.push("================================".to_string());
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
    let mut lines = Vec::new();
    if let Some(target) = &state.current_target {
        lines.push(format!("Target: {target}"));
    }
    if let Some(header) = &state.prompt_header {
        lines.push(format!("Focus: {header}"));
    }
    if state.ir_state.next_allowed_actions.is_empty() {
        if lines.is_empty() {
            lines.push("No active plan.".to_string());
        }
        return lines;
    }
    lines.push(format!(
        "Next: {}",
        state
            .ir_state
            .next_allowed_actions
            .iter()
            .map(|action| action.as_label())
            .collect::<Vec<_>>()
            .join(" / ")
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

fn active_diff_panel(state: &ComposerViewState) -> Option<(String, SessionAppliedDiff)> {
    if let Some(review) = &state.review
        && let Some(diff) = review.preview_diff_snapshot()
    {
        return Some(("[DIFF] Preview".to_string(), diff));
    }
    active_diff_ref(state)
        .cloned()
        .map(|diff| ("[DIFF] Latest Applied".to_string(), diff))
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
        .execution_result
        .clone()
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
    let count = state.transcript.len();
    let latest = state.transcript.last().cloned().unwrap_or_default();
    let summary = if count == 0 {
        "No detail logs.".to_string()
    } else {
        format!("{count} detail line(s) available.")
    };
    let preview = if latest.is_empty() {
        "Press Ctrl+L to expand detail.".to_string()
    } else {
        format!("Latest: {}", latest.lines().next().unwrap_or_default())
    };
    vec![
        summary,
        preview,
        "Press Ctrl+L to expand detail.".to_string(),
    ]
}

fn detail_lines_expanded(state: &ComposerViewState) -> Vec<String> {
    if state.transcript.is_empty() {
        return vec![
            "No detail logs.".to_string(),
            "Press Ctrl+L to collapse detail.".to_string(),
        ];
    }
    let mut lines = state
        .transcript
        .iter()
        .rev()
        .take(6)
        .rev()
        .cloned()
        .collect::<Vec<_>>();
    lines.push("Press Ctrl+L to collapse detail.".to_string());
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
        Paragraph::new(if state.buffer.is_blank() {
            "> Type a command (analyze / refactor / apply)".to_string()
        } else {
            state.buffer.text()
        })
        .style(if state.buffer.is_blank() {
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
        let (row, col) = state.buffer.cursor();
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
    use crate::service::dto::{ActionKind, IRActiveTransaction, SessionAppliedFileDiff};

    use super::*;

    #[test]
    fn composer_buffer_appends_at_tail_and_keeps_cursor_in_sync() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.buffer.insert_str("abc");
        assert_eq!(state.buffer.text(), "abc");
        assert_eq!(state.buffer.cursor(), (0, 3));
    }

    #[test]
    fn wildcard_token_in_prose_does_not_mutate_prompt_state() {
        let mut conversation = ConversationState {
            last_target: Some(PathBuf::from("apps/cli/src/coding.rs")),
            ..ConversationState::default()
        };
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.sync_context(&conversation, Some("main".to_string()), None);
        let header_before = state.prompt_header.clone();
        let target_before = state.current_target.clone();

        state
            .buffer
            .insert_str("ImportRebinding-only の diff では *_interface.rs を生成しない。");
        state.sync_buffer_metadata();

        assert_eq!(state.prompt_header, header_before);
        assert_eq!(state.current_target, target_before);
        assert!(state.file_chips.is_empty(), "{:?}", state.file_chips);

        conversation.last_target = Some(PathBuf::from("apps/cli/src/coding.rs"));
        state.sync_context(&conversation, Some("main".to_string()), None);
        assert_eq!(state.current_target, target_before);
    }

    #[test]
    fn single_line_spec_executes_only_on_submit() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        let mut executions = Vec::new();

        state.buffer.insert_str(
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
        assert!(state.buffer.is_blank());
    }

    #[test]
    fn typing_updates_mode_metadata_without_dispatch() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.focus = ComposerFocus::SendButton;
        let transcript_before = state.transcript.clone();

        for ch in ['@', 'f', 'i', 'l', 'e'] {
            assert_eq!(
                state.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
                ComposerAction::None
            );
        }

        assert_eq!(state.transcript, transcript_before);
        assert_eq!(state.focus, ComposerFocus::SendButton);
        assert!(state.buffer.is_blank());
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
        assert_eq!(state.buffer.cursor(), (0, 0));
    }

    #[test]
    fn transcript_append_filters_debug_leak_prefixes() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.push_transcript_line("visible\nTRACE:R1:ENTER\nDEBUG:hook\nstill visible");
        assert_eq!(state.transcript, vec!["visible\nstill visible".to_string()]);
    }

    #[test]
    fn tab_shift_tab_and_esc_never_leave_editor_deadlocked() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.buffer.insert_str("/command");

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
        let transcript_before = state.transcript.clone();
        for ch in "analyze src/".chars() {
            assert_eq!(
                state.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
                ComposerAction::None
            );
        }

        assert_eq!(state.transcript, transcript_before);

        let first = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        let ComposerAction::Submit(event) = first else {
            panic!("expected submit on enter");
        };
        assert_eq!(event.input, "analyze src/");
        assert!(state.buffer.is_blank());

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
        assert!(state.buffer.is_blank());
    }

    #[test]
    fn backspace_removes_from_tail_only() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.buffer.insert_str("abc");

        assert_eq!(
            state.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
            ComposerAction::None
        );
        assert_eq!(state.buffer.text(), "ab");
        assert_eq!(state.buffer.cursor(), (0, 2));
    }

    #[test]
    fn cursor_invariant_holds_after_supported_operations() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        for ch in "hello".chars() {
            assert_eq!(
                state.handle_key_event(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)),
                ComposerAction::None
            );
            assert_eq!(state.buffer.cursor().1, state.buffer.text().chars().count());
        }

        assert_eq!(
            state.handle_key_event(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE)),
            ComposerAction::None
        );
        assert_eq!(state.buffer.cursor().1, state.buffer.text().chars().count());

        let action = state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(action, ComposerAction::Submit(_)));
        assert_eq!(state.buffer.cursor().1, state.buffer.text().chars().count());
    }

    #[test]
    fn plan_uses_ir_next_actions_only() {
        let mut state = ComposerViewState::new(vec!["existing".to_string()], State::Idle);
        state.ir_state.next_allowed_actions = vec![ActionKind::Apply, ActionKind::Validate];

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
        });

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
