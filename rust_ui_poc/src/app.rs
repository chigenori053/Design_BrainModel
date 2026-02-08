use anyhow::Result;

use crate::model::{
    AppState,
    ActiveTab,
    HumanOverrideLogEntry,
    PhaseCState,
    L1AtomVm,
    DesignDraftVm,
};
use crate::vm_client::VmClient;
use std::fs::{self, OpenOptions};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Represents actions that can be dispatched to the App.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Quit,
    Tick,
    EnterInput,
    ExitInput,
    InputChar(char),
    Backspace,
    SubmitInput,
    CycleTab,
    SelectNextUnit,
    SelectPrevUnit,
    TogglePhaseC,
    SelectNextProposal,
    SelectPrevProposal,
    EnterOverrideInput,
    ExitOverrideInput,
    OverrideInputChar(char),
    OverrideBackspace,
    OverrideAccept,
    OverrideHold,
    OverrideReject,
    OverrideSaveAsKnowledge,
    ToggleHelp,
}

pub struct App {
    pub state: AppState,
    #[allow(dead_code)]
    vm_client: VmClient,
}

impl App {
    pub fn new() -> Self {
        Self {
            state: AppState::new(),
            vm_client: VmClient::new(),
        }
    }

    pub fn init(&mut self) -> Result<()> {
        self.state.logs.push("Application initialized.".to_string());
        self.try_load_phasec_state("data/phasec_state.json");
        Ok(())
    }

    pub fn dispatch(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Quit => self.state.quit(),
            Action::EnterInput => {
                self.state.input_mode = true;
                self.state.logs.push("Input mode enabled.".to_string());
            }
            Action::ExitInput => {
                self.state.input_mode = false;
                self.state.logs.push("Input mode disabled.".to_string());
            }
            Action::InputChar(ch) => self.state.input_buffer.push(ch),
            Action::Backspace => { self.state.input_buffer.pop(); }
            Action::SubmitInput => {
                let content = self.state.input_buffer.trim().to_string();
                if !content.is_empty() {
                    self.submit_ui_input(content)?;
                    self.state.input_buffer.clear();
                }
            }
            Action::CycleTab => {
                self.state.active_tab = match self.state.active_tab {
                    ActiveTab::FreeNote => ActiveTab::Understanding,
                    ActiveTab::Understanding => ActiveTab::DesignDraft,
                    ActiveTab::DesignDraft => ActiveTab::FreeNote,
                };
            }
            Action::SelectNextUnit => self.select_next_unit(),
            Action::SelectPrevUnit => self.select_prev_unit(),
            Action::ToggleHelp => self.state.show_help = !self.state.show_help,
            Action::TogglePhaseC => {
                if self.state.active_view == crate::model::ActiveView::PhaseC {
                    self.state.active_view = crate::model::ActiveView::Normal;
                } else {
                    self.state.active_view = crate::model::ActiveView::PhaseC;
                    self.state.input_mode = false;
                }
            }
            Action::SelectNextProposal => {
                if let Some(state) = &self.state.phasec_state {
                    if !state.proposals.is_empty() {
                        self.state.selected_proposal_index = (self.state.selected_proposal_index + 1) % state.proposals.len();
                    }
                }
            }
            Action::SelectPrevProposal => {
                if let Some(state) = &self.state.phasec_state {
                    if !state.proposals.is_empty() {
                        if self.state.selected_proposal_index == 0 {
                            self.state.selected_proposal_index = state.proposals.len() - 1;
                        } else {
                            self.state.selected_proposal_index -= 1;
                        }
                    }
                }
            }
            Action::EnterOverrideInput => {
                self.state.override_input_mode = true;
                self.state.override_buffer.clear();
            }
            Action::ExitOverrideInput => self.state.override_input_mode = false,
            Action::OverrideInputChar(ch) => self.state.override_buffer.push(ch),
            Action::OverrideBackspace => { self.state.override_buffer.pop(); }
            Action::OverrideAccept => self.log_override_action("ACCEPT")?,
            Action::OverrideHold => self.log_override_action("HOLD")?,
            Action::OverrideReject => self.log_override_action("REJECT")?,
            Action::OverrideSaveAsKnowledge => self.log_override_action("SAVE_AS_KNOWLEDGE")?,
            Action::Tick => {}
        }
        Ok(())
    }

    fn select_next_unit(&mut self) {
        match self.state.active_tab {
            ActiveTab::FreeNote => {}
            ActiveTab::Understanding => {
                if !self.state.l1_atoms.is_empty() {
                    let next = match self.state.selected_l1_index {
                        Some(idx) => (idx + 1) % self.state.l1_atoms.len(),
                        None => 0,
                    };
                    self.state.selected_l1_index = Some(next);
                }
            }
            ActiveTab::DesignDraft => {
                if !self.state.l2_units.is_empty() {
                    let next = match self.state.selected_l2_index {
                        Some(idx) => (idx + 1) % self.state.l2_units.len(),
                        None => 0,
                    };
                    self.state.selected_l2_index = Some(next);
                }
            }
        }
    }

    fn select_prev_unit(&mut self) {
        match self.state.active_tab {
            ActiveTab::FreeNote => {}
            ActiveTab::Understanding => {
                if !self.state.l1_atoms.is_empty() {
                    let prev = match self.state.selected_l1_index {
                        Some(idx) => if idx == 0 { self.state.l1_atoms.len() - 1 } else { idx - 1 },
                        None => self.state.l1_atoms.len() - 1,
                    };
                    self.state.selected_l1_index = Some(prev);
                }
            }
            ActiveTab::DesignDraft => {
                if !self.state.l2_units.is_empty() {
                    let prev = match self.state.selected_l2_index {
                        Some(idx) => if idx == 0 { self.state.l2_units.len() - 1 } else { idx - 1 },
                        None => self.state.l2_units.len() - 1,
                    };
                    self.state.selected_l2_index = Some(prev);
                }
            }
        }
    }

    fn submit_ui_input(&mut self, content: String) -> Result<()> {
        let tab = match self.state.active_tab {
            ActiveTab::FreeNote => "FREE_NOTE",
            ActiveTab::Understanding => "UNDERSTANDING",
            ActiveTab::DesignDraft => "DESIGN_DRAFT",
        };
        let context_id = match self.state.active_tab {
            ActiveTab::Understanding => self.state.selected_l1_index.and_then(|idx| {
                self.state.l1_atoms.get(idx).map(|l1| l1.id.clone())
            }),
            ActiveTab::DesignDraft => self.state.selected_l2_index.and_then(|idx| {
                self.state.l2_units.get(idx).map(|l2| l2.id.clone())
            }),
            _ => None,
        };

        let user_line = format!("You: {}", content);
        self.push_tab_message(self.state.active_tab, user_line);

        match self.vm_client.submit_ui_input(tab, &content, context_id) {
            Ok(value) => {
                if let Some(message) = value.get("message").and_then(|v| v.as_str()) {
                    self.push_tab_message(self.state.active_tab, format!("System: {}", message));
                }
                if let Some(l1_val) = value.get("l1") {
                    if let Ok(l1) = serde_json::from_value::<L1AtomVm>(l1_val.clone()) {
                        self.state.l1_atoms.push(l1);
                        if self.state.selected_l1_index.is_none() {
                            self.state.selected_l1_index = Some(self.state.l1_atoms.len() - 1);
                        }
                    }
                }
                if let Some(draft_val) = value.get("draft") {
                    if let Ok(draft) = serde_json::from_value::<DesignDraftVm>(draft_val.clone()) {
                        self.state.l2_units.push(draft);
                        if self.state.selected_l2_index.is_none() {
                            self.state.selected_l2_index = Some(self.state.l2_units.len() - 1);
                        }
                    }
                }
                if self.state.active_tab == ActiveTab::FreeNote {
                    self.state.free_notes.push(content);
                }
            }
            Err(err) => {
                self.push_tab_message(self.state.active_tab, format!("System: {}", err));
            }
        }
        Ok(())
    }

    fn push_tab_message(&mut self, tab: ActiveTab, message: String) {
        self.state.tab_messages.entry(tab).or_default().push(message);
    }

    fn try_load_phasec_state(&mut self, path: &str) {
        if !Path::new(path).exists() { return; }
        if let Ok(raw) = fs::read_to_string(path) {
            if let Ok(state) = serde_json::from_str::<PhaseCState>(&raw) {
                self.state.phasec_state = Some(state);
                self.state.active_view = crate::model::ActiveView::PhaseC;
            }
        }
    }

    fn log_override_action(&mut self, action: &str) -> Result<()> {
        let state = match &self.state.phasec_state {
            Some(state) => state,
            None => return Ok(()),
        };
        if state.proposals.is_empty() { return Ok(()); }

        let index = self.state.selected_proposal_index.min(state.proposals.len() - 1);
        let target_id = state.proposals[index].id.clone();
        let rationale = if self.state.override_buffer.trim().is_empty() { None } else { Some(self.state.override_buffer.trim().to_string()) };
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();

        let entry = HumanOverrideLogEntry { timestamp, target_id, action: action.to_string(), rationale };
        self.state.override_logs.push(entry.clone());
        let _ = self.append_override_log(&entry);
        self.state.override_buffer.clear();
        self.state.override_input_mode = false;
        Ok(())
    }

    fn append_override_log(&mut self, entry: &HumanOverrideLogEntry) -> Result<()> {
        let dir = Path::new("rust_ui_poc/logs");
        if !dir.exists() { fs::create_dir_all(dir)?; }
        let path = dir.join("human_override.jsonl");
        let line = serde_json::to_string(entry).unwrap_or_else(|_| "{}".to_string());
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        use std::io::Write;
        writeln!(file, "{}", line)?;
        Ok(())
    }
}
