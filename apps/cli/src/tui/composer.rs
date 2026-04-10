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
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
}

impl ComposerBuffer {
    pub fn new() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
        }
    }

    pub fn text(&self) -> String {
        self.lines.join("\n")
    }

    pub fn is_blank(&self) -> bool {
        self.lines.iter().all(|line| line.trim().is_empty())
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col)
    }

    pub fn move_cursor_to_end(&mut self) {
        self.cursor_row = self.lines.len().saturating_sub(1);
        self.cursor_col = self.lines[self.cursor_row].chars().count();
    }

    pub fn insert_char(&mut self, ch: char) {
        let col = self.current_line_byte_col();
        self.lines[self.cursor_row].insert(col, ch);
        self.cursor_col += 1;
    }

    pub fn insert_str(&mut self, value: &str) {
        for ch in value.chars() {
            self.insert_char(ch);
        }
    }

    pub fn insert_newline(&mut self) {
        let split = self.current_line_byte_col();
        let tail = self.lines[self.cursor_row].split_off(split);
        self.cursor_row += 1;
        self.cursor_col = 0;
        self.lines.insert(self.cursor_row, tail);
    }

    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            let remove_at = self.current_line_byte_col_before_cursor();
            self.lines[self.cursor_row].remove(remove_at);
            self.cursor_col -= 1;
            return;
        }
        if self.cursor_row == 0 {
            return;
        }
        let current = self.lines.remove(self.cursor_row);
        self.cursor_row -= 1;
        self.cursor_col = self.lines[self.cursor_row].chars().count();
        self.lines[self.cursor_row].push_str(&current);
    }

    pub fn delete(&mut self) {
        let line_len = self.lines[self.cursor_row].chars().count();
        if self.cursor_col < line_len {
            let idx = self.current_line_byte_col();
            self.lines[self.cursor_row].remove(idx);
            return;
        }
        if self.cursor_row + 1 >= self.lines.len() {
            return;
        }
        let next = self.lines.remove(self.cursor_row + 1);
        self.lines[self.cursor_row].push_str(&next);
    }

    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].chars().count();
        }
    }

    pub fn move_right(&mut self) {
        let line_len = self.lines[self.cursor_row].chars().count();
        if self.cursor_col < line_len {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor_row == 0 {
            return;
        }
        self.cursor_row -= 1;
        self.cursor_col = self
            .cursor_col
            .min(self.lines[self.cursor_row].chars().count());
    }

    pub fn move_down(&mut self) {
        if self.cursor_row + 1 >= self.lines.len() {
            return;
        }
        self.cursor_row += 1;
        self.cursor_col = self
            .cursor_col
            .min(self.lines[self.cursor_row].chars().count());
    }

    pub fn submit_document(&mut self, eof_submit: bool) -> Option<SubmitEvent> {
        let input = self.text();
        if input.trim().is_empty() {
            return None;
        }
        *self = Self::new();
        Some(SubmitEvent { input, eof_submit })
    }

    fn current_line_byte_col(&self) -> usize {
        char_to_byte_index(&self.lines[self.cursor_row], self.cursor_col)
    }

    fn current_line_byte_col_before_cursor(&self) -> usize {
        char_to_byte_index(
            &self.lines[self.cursor_row],
            self.cursor_col.saturating_sub(1),
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ComposerViewState {
    pub transcript: Vec<String>,
    pub review: Option<ReviewBatchState>,
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
        self.sync_buffer_metadata();
    }

    pub fn push_transcript_line(&mut self, line: impl Into<String>) {
        self.transcript.push(line.into());
        self.restore_intent_document_focus();
    }

    pub fn activate_review(&mut self, review: ReviewBatchState) {
        self.review = Some(review);
        self.mode = ComposerUiMode::PatchReview;
        self.restore_intent_document_focus();
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
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.buffer.insert_newline();
                self.sync_buffer_metadata();
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
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Esc => {
                self.restore_intent_document_focus();
                ComposerAction::None
            }
            KeyCode::Backspace => {
                self.buffer.backspace();
                self.sync_buffer_metadata();
                ComposerAction::None
            }
            KeyCode::Delete => {
                self.buffer.delete();
                self.sync_buffer_metadata();
                ComposerAction::None
            }
            KeyCode::Left => {
                self.buffer.move_left();
                ComposerAction::None
            }
            KeyCode::Right => {
                self.buffer.move_right();
                ComposerAction::None
            }
            KeyCode::Up => {
                self.buffer.move_up();
                ComposerAction::None
            }
            KeyCode::Down => {
                self.buffer.move_down();
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
            Constraint::Min(6),
            Constraint::Length(8),
            Constraint::Length(2),
        ])
        .split(area);

    render_context_bar(frame, state, vertical[0]);
    render_proc_strip(frame, &state.proc_strip, vertical[1]);
    if let Some(review) = &state.review {
        render_edit_blocks(frame, review, vertical[2]);
    }
    render_transcript(frame, state, vertical[3]);
    render_composer_panel(frame, state, vertical[4]);
    render_footer_hints(frame, state, vertical[5]);
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

fn render_transcript(frame: &mut Frame, state: &ComposerViewState, area: Rect) {
    let content = if state.transcript.is_empty() {
        vec![Line::from("")]
    } else {
        state
            .transcript
            .iter()
            .rev()
            .take(area.height.saturating_sub(2) as usize)
            .rev()
            .map(|line| Line::from(line.clone()))
            .collect::<Vec<_>>()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(border::ROUNDED)
        .title(" Transcript ")
        .border_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(
        Paragraph::new(content)
            .block(block)
            .wrap(Wrap { trim: false }),
        area,
    );
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
        Paragraph::new(state.buffer.lines().join("\n"))
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
        "Shift+Enter inserts newline"
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
        Span::styled("Shift+Enter newline", Style::default().fg(Color::Yellow)),
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

fn char_to_byte_index(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map(|(idx, _)| idx)
        .unwrap_or(value.len())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::nl::session::ConversationState;

    use super::*;

    #[test]
    fn composer_shift_enter_preserves_multiline_intent() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.buffer.insert_str("first line");
        assert_eq!(
            state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)),
            ComposerAction::None
        );
        state.buffer.insert_str("second line");

        let submit = state.buffer.submit_document(false).expect("submit event");
        assert_eq!(submit.input, "first line\nsecond line");
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
    fn multiline_spec_executes_only_on_submit() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        let mut executions = Vec::new();

        state
            .buffer
            .insert_str("Semantic Interface Extraction Guard を追加する。");
        assert!(executions.is_empty());
        state.buffer.insert_newline();
        state.buffer.insert_str("対象は apps/cli/src/coding.rs。");
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
            "Semantic Interface Extraction Guard を追加する。\n対象は apps/cli/src/coding.rs。"
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
        assert_eq!(state.buffer.text(), "@file");
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
        assert!(state.file_chips.iter().any(|chip| chip == "@file"));
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
    fn shift_enter_only_inserts_newline() {
        let mut state = ComposerViewState::new(Vec::new(), State::Idle);
        state.buffer.insert_str("line1");
        let transcript_before = state.transcript.clone();

        assert_eq!(
            state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT)),
            ComposerAction::None
        );

        assert_eq!(state.transcript, transcript_before);
        assert_eq!(state.buffer.text(), "line1\n");
    }
}
