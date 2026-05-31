use std::collections::VecDeque;
use std::io;
use std::time::SystemTime;

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{
        Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode,
    },
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::specification_bridge::{
    DiagnosisDomain, ImplementationPlan, ImplementationPlanner, RepairPlan, RepairPlanner,
    SpecificationContext, SpecificationKind, StructuralDiagnosisRequest, StructuralDiagnosisResult,
    classify_specification,
};

pub const MAX_EVENTS: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEventCategory {
    Specification,
    Diagnosis,
    UiDiagnosis,
    RepairPlan,
    UiRepairPlan,
    ImplementationPlan,
    UiImplementationPlan,
    Runtime,
    Audit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiEvent {
    pub timestamp: SystemTime,
    pub category: UiEventCategory,
    pub message: String,
}

impl UiEvent {
    pub fn new(category: UiEventCategory, message: impl Into<String>) -> Self {
        Self {
            timestamp: SystemTime::now(),
            category,
            message: message.into(),
        }
    }

    pub fn label(&self) -> &'static str {
        match self.category {
            UiEventCategory::Specification => "SPEC_CONTEXT",
            UiEventCategory::Diagnosis => "STRUCTURAL_DIAGNOSIS",
            UiEventCategory::UiDiagnosis => "UI_DIAGNOSIS",
            UiEventCategory::RepairPlan => "REPAIR_PLAN",
            UiEventCategory::UiRepairPlan => "UI_REPAIR_PLAN",
            UiEventCategory::ImplementationPlan => "IMPLEMENTATION_PLAN",
            UiEventCategory::UiImplementationPlan => "UI_IMPLEMENTATION_PLAN",
            UiEventCategory::Runtime => "RUNTIME",
            UiEventCategory::Audit => "AUDIT",
        }
    }

    pub fn render_lines(&self) -> Vec<String> {
        self.message
            .lines()
            .enumerate()
            .map(|(idx, line)| {
                if idx == 0 {
                    format!("[{}] {}", self.label(), line)
                } else {
                    format!("{}{}", " ".repeat(self.label().len() + 3), line)
                }
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiEventStore {
    events: VecDeque<UiEvent>,
}

impl Default for UiEventStore {
    fn default() -> Self {
        Self {
            events: VecDeque::new(),
        }
    }
}

impl UiEventStore {
    pub fn push(&mut self, event: UiEvent) {
        self.events.push_back(event);
        while self.events.len() > MAX_EVENTS {
            self.events.pop_front();
        }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &UiEvent> {
        self.events.iter()
    }
}

pub trait UiEventSink {
    fn emit(&mut self, event: UiEvent);
}

impl UiEventSink for UiEventStore {
    fn emit(&mut self, event: UiEvent) {
        self.push(event);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TuiState {
    pub input_buffer: String,
    pub output_scroll: usize,
    pub events: UiEventStore,
    cursor: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TuiAction {
    None,
    Submit(String),
    Quit,
}

impl TuiState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn output_lines(&self) -> Vec<String> {
        self.events
            .iter()
            .flat_map(UiEvent::render_lines)
            .collect::<Vec<_>>()
    }

    fn handle_key_event(&mut self, key: KeyEvent) -> TuiAction {
        if key.kind != KeyEventKind::Press {
            return TuiAction::None;
        }

        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => TuiAction::Quit,
            KeyCode::Esc => TuiAction::Quit,
            KeyCode::Enter => self.handle_enter(),
            KeyCode::Backspace => {
                self.backspace();
                TuiAction::None
            }
            KeyCode::Up | KeyCode::PageUp => {
                self.output_scroll = self.output_scroll.saturating_add(5);
                TuiAction::None
            }
            KeyCode::Down | KeyCode::PageDown => {
                self.output_scroll = self.output_scroll.saturating_sub(5);
                TuiAction::None
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input_buffer.insert(self.cursor, ch);
                self.cursor += ch.len_utf8();
                TuiAction::None
            }
            _ => TuiAction::None,
        }
    }

    fn handle_enter(&mut self) -> TuiAction {
        let input = self.input_buffer.trim().to_string();
        if input.is_empty() {
            return TuiAction::None;
        }
        if is_repl_command(&input) || self.current_line_is_empty() {
            self.input_buffer.clear();
            self.cursor = 0;
            if input == "/exit" {
                return TuiAction::Quit;
            }
            return TuiAction::Submit(input);
        }
        self.input_buffer.insert(self.cursor, '\n');
        self.cursor += 1;
        TuiAction::None
    }

    fn current_line_is_empty(&self) -> bool {
        self.input_buffer
            .get(..self.cursor)
            .and_then(|prefix| prefix.rsplit('\n').next())
            .is_some_and(|line| line.trim().is_empty())
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        if let Some((idx, _)) = self.input_buffer[..self.cursor].char_indices().next_back() {
            self.input_buffer.replace_range(idx..self.cursor, "");
            self.cursor = idx;
        }
    }
}

pub fn run_phase1_tui() -> Result<(), String> {
    let mut terminal = enter_terminal()?;
    let mut state = TuiState::new();
    state.events.emit(UiEvent::new(
        UiEventCategory::Runtime,
        "DBM TUI ready. Enter a design specification, then press Enter on a blank line.",
    ));

    let result = run_loop(&mut terminal, &mut state);
    restore_terminal(&mut terminal);
    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut TuiState,
) -> Result<(), String> {
    loop {
        terminal
            .draw(|frame| render(frame, state))
            .map_err(|err| err.to_string())?;

        if let Event::Key(key) = event::read().map_err(|err| err.to_string())? {
            match state.handle_key_event(key) {
                TuiAction::None => {}
                TuiAction::Quit => break,
                TuiAction::Submit(input) => dispatch_input(state, &input),
            }
        }
    }
    Ok(())
}

pub fn dispatch_input(state: &mut TuiState, input: &str) {
    let trimmed = input.trim();
    if is_repl_command(trimmed) {
        state.events.emit(UiEvent::new(
            UiEventCategory::Runtime,
            format!("command accepted: {trimmed}"),
        ));
        return;
    }

    match classify_specification(trimmed) {
        SpecificationKind::Instruction => state.events.emit(UiEvent::new(
            UiEventCategory::Runtime,
            "instruction input is reserved for the existing REPL command flow",
        )),
        SpecificationKind::DesignSpecification => {
            if let Err(err) = dispatch_design_specification(trimmed, &mut state.events) {
                state.events.emit(UiEvent::new(
                    UiEventCategory::Audit,
                    format!("rejected: {err}"),
                ));
            }
        }
    }
}

fn dispatch_design_specification(text: &str, sink: &mut dyn UiEventSink) -> Result<(), String> {
    let context = SpecificationContext::from_yaml(text).map_err(|err| err.to_string())?;
    sink.emit(UiEvent::new(
        UiEventCategory::Specification,
        format!(
            "generated\ngoals={}\nconstraints={}\ncomponents={}\nrules={}",
            context.goals.len(),
            context.constraints.len(),
            context.architecture.len(),
            context.rules.len()
        ),
    ));

    let request = StructuralDiagnosisRequest::new(context);
    let domain = request.domain;
    let diagnosis = request.diagnose();
    sink.emit(diagnosis_event(&diagnosis, domain));

    let repair_plan = RepairPlanner::generate(&diagnosis);
    sink.emit(repair_plan_event(&repair_plan, domain));

    let implementation_plan = ImplementationPlanner::generate(&repair_plan);
    sink.emit(implementation_plan_event(&implementation_plan, domain));
    Ok(())
}

fn diagnosis_event(diagnosis: &StructuralDiagnosisResult, domain: DiagnosisDomain) -> UiEvent {
    let mut lines = vec![format!(
        "violations={} warnings={}",
        diagnosis.violations.len(),
        diagnosis.warnings.len()
    )];
    lines.push("Violations:".to_string());
    if diagnosis.violations.is_empty() {
        lines.push("- none".to_string());
    } else {
        lines.extend(
            diagnosis
                .violations
                .iter()
                .map(|v| format!("- {}: {}", v.rule, v.message)),
        );
    }
    lines.push("Warnings:".to_string());
    if diagnosis.warnings.is_empty() {
        lines.push("- none".to_string());
    } else {
        lines.extend(
            diagnosis
                .warnings
                .iter()
                .map(|w| format!("- {}: {}", w.rule, w.message)),
        );
    }
    let category = if domain == DiagnosisDomain::UserInterface {
        UiEventCategory::UiDiagnosis
    } else {
        UiEventCategory::Diagnosis
    };
    UiEvent::new(category, lines.join("\n"))
}

fn repair_plan_event(plan: &RepairPlan, domain: DiagnosisDomain) -> UiEvent {
    let mut lines = vec![format!(
        "suggestions={} steps={}",
        plan.suggestions.len(),
        plan.execution_steps.len()
    )];
    if plan.suggestions.is_empty() {
        lines.push("- no repair required".to_string());
    } else {
        lines.extend(plan.suggestions.iter().map(|suggestion| {
            format!(
                "- {} [{}]: {}",
                suggestion.title, suggestion.priority, suggestion.rationale
            )
        }));
    }
    if !plan.execution_steps.is_empty() {
        lines.push("Steps:".to_string());
        lines.extend(
            plan.execution_steps
                .iter()
                .map(|step| format!("{}. {}", step.order, step.description)),
        );
    }
    let category = if domain == DiagnosisDomain::UserInterface {
        UiEventCategory::UiRepairPlan
    } else {
        UiEventCategory::RepairPlan
    };
    UiEvent::new(category, lines.join("\n"))
}

fn implementation_plan_event(plan: &ImplementationPlan, domain: DiagnosisDomain) -> UiEvent {
    let mut lines = vec![format!(
        "tasks={} files={} validations={}",
        plan.tasks.len(),
        plan.file_modifications.len(),
        plan.validations.len()
    )];
    if plan.tasks.is_empty() {
        lines.push("- no implementation tasks required".to_string());
    } else {
        lines.push("Tasks:".to_string());
        lines.extend(plan.tasks.iter().map(|task| {
            format!(
                "- {} [{}] target={}",
                task.title,
                implementation_priority_label(task.priority),
                task.target_component
            )
        }));
    }
    if !plan.validations.is_empty() {
        lines.push("Validations:".to_string());
        lines.extend(
            plan.validations
                .iter()
                .map(|validation| format!("- {}", validation.description)),
        );
    }
    let category = if domain == DiagnosisDomain::UserInterface {
        UiEventCategory::UiImplementationPlan
    } else {
        UiEventCategory::ImplementationPlan
    };
    UiEvent::new(category, lines.join("\n"))
}

fn implementation_priority_label(
    priority: crate::specification_bridge::ImplementationPriority,
) -> &'static str {
    match priority {
        crate::specification_bridge::ImplementationPriority::Critical => "Critical",
        crate::specification_bridge::ImplementationPriority::High => "High",
        crate::specification_bridge::ImplementationPriority::Normal => "Normal",
        crate::specification_bridge::ImplementationPriority::Low => "Low",
    }
}

pub fn event_from_log_line(line: &str) -> Option<UiEvent> {
    let trimmed = line.trim();
    let category = if trimmed.starts_with("[SPEC_CONTEXT]") {
        UiEventCategory::Specification
    } else if trimmed.starts_with("[UI_DIAGNOSIS]") {
        UiEventCategory::UiDiagnosis
    } else if trimmed.starts_with("[STRUCTURAL_DIAGNOSIS]") {
        UiEventCategory::Diagnosis
    } else if trimmed.starts_with("[UI_REPAIR_PLAN]") {
        UiEventCategory::UiRepairPlan
    } else if trimmed.starts_with("[REPAIR_PLAN]") {
        UiEventCategory::RepairPlan
    } else if trimmed.starts_with("[UI_IMPLEMENTATION_PLAN]") {
        UiEventCategory::UiImplementationPlan
    } else if trimmed.starts_with("[IMPLEMENTATION_PLAN]") {
        UiEventCategory::ImplementationPlan
    } else {
        return None;
    };
    Some(UiEvent::new(
        category,
        trimmed
            .split_once(']')
            .map(|(_, rest)| rest.trim().to_string())
            .unwrap_or_default(),
    ))
}

fn is_repl_command(input: &str) -> bool {
    matches!(
        input.trim(),
        "/exit" | "select" | "apply" | "rollback" | "y" | "n" | "cancel"
    )
}

fn render(frame: &mut Frame, state: &TuiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(5)])
        .split(frame.area());
    render_output_area(frame, state, chunks[0]);
    render_input_area(frame, state, chunks[1]);
}

pub fn render_output(frame: &mut Frame, state: &TuiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(5)])
        .split(frame.area());
    render_output_area(frame, state, chunks[0]);
}

pub fn render_input(frame: &mut Frame, state: &TuiState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(5)])
        .split(frame.area());
    render_input_area(frame, state, chunks[1]);
}

fn render_output_area(frame: &mut Frame, state: &TuiState, area: ratatui::layout::Rect) {
    let lines = state
        .output_lines()
        .into_iter()
        .map(Line::from)
        .collect::<Vec<_>>();
    let viewport_height = area.height.saturating_sub(2);
    let max_scroll = (lines.len() as u16).saturating_sub(viewport_height);
    let scroll = max_scroll.saturating_sub(state.output_scroll as u16);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" OUTPUT ")
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .scroll((scroll, 0))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_input_area(frame: &mut Frame, state: &TuiState, area: ratatui::layout::Rect) {
    let display = if state.input_buffer.is_empty() {
        "> ".to_string()
    } else {
        format!("> {}", state.input_buffer)
    };
    frame.render_widget(
        Paragraph::new(display)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" INPUT ")
                    .border_style(
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn enter_terminal() -> Result<Terminal<CrosstermBackend<io::Stdout>>, String> {
    enable_raw_mode().map_err(|err| err.to_string())?;
    let mut stdout = io::stdout();
    if let Err(err) = execute!(
        stdout,
        EnterAlternateScreen,
        Hide,
        Clear(ClearType::All),
        MoveTo(0, 0)
    ) {
        disable_raw_mode().ok();
        return Err(err.to_string());
    }
    Terminal::new(CrosstermBackend::new(stdout)).map_err(|err| err.to_string())
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) {
    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        Show,
        Clear(ClearType::All),
        MoveTo(0, 0),
        LeaveAlternateScreen
    )
    .ok();
    terminal.show_cursor().ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> &'static str {
        r#"system_name: DBM_REPL_UI
rules:
  - Runtime must pass through AuditCore
  - ApplyGate required
"#
    }

    #[test]
    fn event_store_enforces_fifo_limit() {
        let mut store = UiEventStore::default();
        for idx in 0..(MAX_EVENTS + 3) {
            store.push(UiEvent::new(UiEventCategory::Runtime, idx.to_string()));
        }

        assert_eq!(store.len(), MAX_EVENTS);
        assert_eq!(store.iter().next().unwrap().message, "3");
    }

    #[test]
    fn log_adapter_routes_spec_context() {
        let event = event_from_log_line("[SPEC_CONTEXT] generated").expect("event");

        assert_eq!(event.category, UiEventCategory::Specification);
        assert_eq!(event.message, "generated");
    }

    #[test]
    fn design_specification_pipeline_routes_to_output_events() {
        let mut state = TuiState::new();
        dispatch_input(&mut state, spec());
        let lines = state.output_lines().join("\n");

        assert!(lines.contains("[SPEC_CONTEXT] generated"));
        assert!(lines.contains("[STRUCTURAL_DIAGNOSIS]"));
        assert!(lines.contains("[REPAIR_PLAN]"));
        assert!(lines.contains("[IMPLEMENTATION_PLAN]"));
    }

    #[test]
    fn repl_compatible_command_routes_as_runtime_event() {
        let mut state = TuiState::new();
        dispatch_input(&mut state, "rollback");

        let lines = state.output_lines().join("\n");
        assert!(lines.contains("[RUNTIME] command accepted: rollback"));
    }
}
