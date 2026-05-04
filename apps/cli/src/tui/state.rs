use std::collections::VecDeque;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use strategy_engine::ExecutionPlanCandidate;

pub use crate::core::{Constraint, DesignDocument, ReasonUnit, StructureTree};
use crate::pipeline::PipelineState;

use super::model::UiPayload;

pub const MAX_CHAT_LINES: usize = 1000;
pub const MAX_EVENTS: usize = 2000;
pub const DESIGN_MAX_LINES: usize = 20;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UiEvent {
    Thinking {
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
    Result {
        message: String,
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
    /// Structured execution proposal.  Spec DBM-EXECUTION-CANDIDATE-SPEC §9.
    Proposal {
        candidates: Vec<ExecutionPlanCandidate>,
    },
}

impl UiEvent {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Thinking { .. } => "THINKING",
            Self::Editing { .. } => "EDITING",
            Self::Plan { .. } => "PLAN",
            Self::Execution { .. } => "EXECUTION",
            Self::Preview { .. } => "PREVIEW",
            Self::Result { .. } => "RESULT",
            Self::Pipeline { .. } => "PIPELINE",
            Self::Next { .. } => "NEXT",
            Self::Error { .. } => "ERROR",
            Self::Debug { .. } => "DEBUG",
            Self::Proposal { .. } => "PROPOSAL",
        }
    }

    pub fn text(&self) -> String {
        match self {
            Self::Thinking { summary } => summary.clone(),
            Self::Editing { target, action } => format!("{target}: {action}"),
            Self::Plan { steps } => steps.join("\n"),
            Self::Execution { step } => step.clone(),
            Self::Preview { diff } => diff.join("\n"),
            Self::Result { message } | Self::Error { message } | Self::Debug { message } => {
                message.clone()
            }
            Self::Pipeline { state } => state.clone(),
            Self::Next { actions } => actions.join("\n"),
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

#[derive(Debug, Clone, Default, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
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

impl Default for InputBuffer {
    fn default() -> Self {
        Self {
            text: String::new(),
            cursor: 0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChatState {
    pub events: Vec<UiEvent>,
}

impl ChatState {
    pub fn append_chat(&mut self, event: UiEvent) {
        self.events.push(event);
        while self.events.len() > MAX_CHAT_LINES {
            self.events.remove(0);
        }
    }
}

#[derive(Debug, Clone)]
pub struct TuiState {
    pub chat: ChatState,
    pub design_doc: DesignDocument,
    pub input: InputBuffer,
    pub focus: Focus,
    pub chat_scroll: ChatScrollState,
    pub event_queue: EventQueue,
    pub pipeline_state: PipelineState,
    pub design_scroll: usize,
    pub design_collapsed: bool,
    pub design_updated: bool,
    pub history: Vec<String>,
    history_cursor: Option<usize>,
    /// Active proposal candidates awaiting `select <n>`.  Phase 1C.5 §4.1.
    /// Set when a `UiEvent::Proposal` arrives; cleared on successful select.
    pub current_proposals: Option<Vec<ExecutionPlanCandidate>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiAction {
    None,
    Quit,
    Submit(String),
}

impl TuiState {
    pub fn new(payload: UiPayload) -> Self {
        Self {
            chat: ChatState {
                events: seed_chat_stream(&payload),
            },
            design_doc: seed_design_document(&payload),
            input: InputBuffer::default(),
            focus: Focus::Input,
            chat_scroll: ChatScrollState::default(),
            event_queue: EventQueue::default(),
            pipeline_state: PipelineState::default(),
            design_scroll: 0,
            design_collapsed: false,
            design_updated: false,
            history: Vec::new(),
            history_cursor: None,
            current_proposals: None,
        }
        .with_pseudo_stream()
    }

    pub fn handle_key_event(&mut self, key: KeyEvent) -> TuiAction {
        match key.code {
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return TuiAction::Quit;
            }
            KeyCode::Esc => {
                if self.focus == Focus::Input && !self.input.text.is_empty() {
                    self.input.clear();
                    return TuiAction::None;
                }
                return TuiAction::Quit;
            }
            KeyCode::Tab => {
                self.focus = self.focus.next();
                return TuiAction::None;
            }
            KeyCode::BackTab => {
                self.focus = self.focus.previous();
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
        self.event_queue.push(event);
    }

    pub fn handle_ui_events(&mut self) {
        while let Some(event) = self.event_queue.pop() {
            self.append_chat(event);
        }
    }

    pub fn append_chat(&mut self, event: UiEvent) {
        // Capture proposal candidates for the select command.  Phase 1C.5 §4.1.
        if let UiEvent::Proposal { ref candidates } = event {
            self.current_proposals = Some(candidates.clone());
        }
        self.chat.append_chat(event);
        self.chat_scroll.apply_append();
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

    pub fn flattened_chat_lines(&self) -> Vec<String> {
        self.chat
            .events
            .iter()
            .flat_map(|event| event.lines())
            .collect()
    }

    fn handle_input_key(&mut self, key: KeyEvent) -> TuiAction {
        match key.code {
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                self.input.insert_newline();
                TuiAction::None
            }
            KeyCode::Enter => {
                let submitted = self.input.text.trim().to_string();
                if submitted.is_empty() {
                    return TuiAction::None;
                }
                self.history.push(submitted.clone());
                self.history_cursor = None;
                self.input.clear();
                self.enqueue_event(UiEvent::Next {
                    actions: vec![submitted.clone()],
                });
                TuiAction::Submit(submitted)
            }
            KeyCode::Backspace => {
                self.input.backspace();
                TuiAction::None
            }
            KeyCode::Delete => {
                self.input.delete();
                TuiAction::None
            }
            KeyCode::Left => {
                self.input.move_left();
                TuiAction::None
            }
            KeyCode::Right => {
                self.input.move_right();
                TuiAction::None
            }
            KeyCode::Up => {
                self.history_previous();
                TuiAction::None
            }
            KeyCode::Down => {
                self.history_next();
                TuiAction::None
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.input.line_count() < 3 || ch != '\n' {
                    self.input.insert_char(ch);
                }
                TuiAction::None
            }
            _ => TuiAction::None,
        }
    }

    fn handle_chat_key(&mut self, key: KeyEvent) -> TuiAction {
        match key.code {
            KeyCode::PageUp | KeyCode::Up => {
                self.chat_scroll.user_scroll_up(5);
            }
            KeyCode::PageDown | KeyCode::Down => {
                self.chat_scroll.user_scroll_down(5);
            }
            KeyCode::End => {
                self.chat_scroll.scroll_to_bottom();
            }
            _ => {}
        }
        TuiAction::None
    }

    fn handle_design_key(&mut self, key: KeyEvent) -> TuiAction {
        match key.code {
            KeyCode::PageUp | KeyCode::Up => {
                self.design_scroll = self.design_scroll.saturating_sub(5);
            }
            KeyCode::PageDown | KeyCode::Down => {
                let max = self.design_doc.rendered.len().saturating_sub(1);
                self.design_scroll = (self.design_scroll + 5).min(max);
            }
            KeyCode::Home => {
                self.design_scroll = 0;
            }
            KeyCode::End => {
                self.design_scroll = self.design_doc.rendered.len().saturating_sub(1);
            }
            KeyCode::Char('d') | KeyCode::Char('D') => {
                self.design_collapsed = !self.design_collapsed;
            }
            _ => {}
        }
        TuiAction::None
    }

    fn history_previous(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let idx = self
            .history_cursor
            .map(|idx| idx.saturating_sub(1))
            .unwrap_or_else(|| self.history.len().saturating_sub(1));
        self.history_cursor = Some(idx);
        self.input.set_text(self.history[idx].clone());
    }

    fn history_next(&mut self) {
        let Some(idx) = self.history_cursor else {
            return;
        };
        if idx + 1 >= self.history.len() {
            self.history_cursor = None;
            self.input.clear();
        } else {
            let next = idx + 1;
            self.history_cursor = Some(next);
            self.input.set_text(self.history[next].clone());
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

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    use super::*;
    use crate::tui::model::{ScorePartsViewModel, TraceStatsViewModel, TraceViewModel, UiPayload};

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
    fn input_submit_queues_next_event_and_history() {
        let mut state = TuiState::new(empty_payload());
        for ch in "fix parser bug".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        let action = state.handle_key_event(key(KeyCode::Enter));

        assert_eq!(action, TuiAction::Submit("fix parser bug".to_string()));
        assert_eq!(state.history, vec!["fix parser bug"]);
        assert!(state.input.text.is_empty());
        assert!(!state.event_queue.is_empty());

        state.handle_ui_events();
        assert!(
            state
                .flattened_chat_lines()
                .iter()
                .any(|line| line == "[NEXT] fix parser bug")
        );
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
    fn shift_enter_allows_up_to_three_input_lines() {
        let mut state = TuiState::new(empty_payload());
        state.handle_key_event(key(KeyCode::Char('a')));
        for _ in 0..4 {
            state.handle_key_event(KeyEvent::new(KeyCode::Enter, KeyModifiers::SHIFT));
            state.handle_key_event(key(KeyCode::Char('b')));
        }

        assert_eq!(state.input.line_count(), 3);
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
    fn queued_event_does_not_interfere_with_input_buffer() {
        let mut state = TuiState::new(empty_payload());
        for ch in "typing".chars() {
            state.handle_key_event(key(KeyCode::Char(ch)));
        }

        state.enqueue_event(UiEvent::Thinking {
            summary: "async event".to_string(),
        });
        state.handle_ui_events();

        assert_eq!(state.input.text, "typing");
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
}
