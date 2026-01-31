use anyhow::Result;
use log::error;

use crate::model::{AppState, CreateL1AtomPayload, L1Type};
use crate::vm_client::VmClient;

/// Represents actions that can be dispatched to the App.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// Quit the application.
    Quit,
    /// Placeholder for future use.
    Tick,
    /// Create a new L1 Atom with the given content.
    CreateL1Atom(String),
    /// Enter input mode.
    EnterInput,
    /// Exit input mode.
    ExitInput,
    /// Append a character to the input buffer.
    InputChar(char),
    /// Remove the last character from the input buffer.
    Backspace,
    /// Submit the current input buffer.
    SubmitInput,
    /// Cycle the L1 type.
    CycleL1Type,
    /// Set a specific L1 type.
    SetL1Type(L1Type),
    // Add other actions like SelectNextCluster, EnterInputMode, etc.
}

/// The main application structure, responsible for state management and logic.
pub struct App {
    pub state: AppState,
    vm_client: VmClient,
}

impl App {
    /// Creates a new App instance.
    pub fn new() -> Self {
        Self {
            state: AppState::new(),
            vm_client: VmClient::new(),
        }
    }

    /// Called once on startup.
    pub fn init(&mut self) -> Result<()> {
        self.state.logs.push("Application initialized.".to_string());

        // For now, we won't create data on init, but we could.
        // Instead, we might want to fetch an initial list of clusters.
        // Let's assume an endpoint `/viewmodel/clusters` exists for this.
        // Since it doesn't, we'll log it and proceed with an empty list.
        self.state.logs.push("Skipping initial data fetch (endpoint not implemented).".to_string());
        
        Ok(())
    }

    /// Dispatches an action to be processed, modifying the app's state.
    pub fn dispatch(&mut self, action: Action) -> Result<()> {
        match action {
            Action::Quit => {
                self.state.quit();
            }
            Action::EnterInput => {
                self.state.input_mode = true;
                self.state.logs.push("Input mode enabled.".to_string());
            }
            Action::ExitInput => {
                self.state.input_mode = false;
                self.state.logs.push("Input mode disabled.".to_string());
            }
            Action::InputChar(ch) => {
                self.state.input_buffer.push(ch);
                if !self.state.input_l1_type_manual {
                    self.state.input_l1_type = classify_l1_type(&self.state.input_buffer);
                }
            }
            Action::Backspace => {
                self.state.input_buffer.pop();
                if !self.state.input_l1_type_manual {
                    self.state.input_l1_type = classify_l1_type(&self.state.input_buffer);
                }
            }
            Action::SubmitInput => {
                let content = self.state.input_buffer.trim().to_string();
                if content.is_empty() {
                    self.state.logs.push("Cannot submit empty input.".to_string());
                } else {
                    let l1_type = self.state.input_l1_type;
                    self.state.logs.push(format!(
                        "Submitting L1 ({:?}): {}",
                        l1_type, content
                    ));
                    self.create_l1_atom(content, l1_type)?;
                    self.state.input_buffer.clear();
                    self.state.input_l1_type = L1Type::default();
                    self.state.input_l1_type_manual = false;
                }
            }
            Action::CycleL1Type => {
                self.state.input_l1_type = next_l1_type(self.state.input_l1_type);
                self.state.input_l1_type_manual = true;
            }
            Action::SetL1Type(l1_type) => {
                self.state.input_l1_type = l1_type;
                self.state.input_l1_type_manual = true;
            }
            Action::CreateL1Atom(content) => {
                self.state.logs.push(format!("Dispatching CreateL1Atom with content: {}", content));
                self.create_l1_atom(content, L1Type::Question)?;
            }
            Action::Tick => {
                // This action can be used for periodic updates, e.g., fetching data.
                // For now, we'll just log it.
                // info!("Tick received");
            }
        }
        Ok(())
    }

    fn create_l1_atom(&mut self, content: String, l1_type: L1Type) -> Result<()> {
        let payload = CreateL1AtomPayload {
            l1_type: format!("{:?}", l1_type).to_uppercase(),
            content,
            source: "human_text_ui".to_string(),
            context_id: None,
        };
        match self.vm_client.execute_command("CreateL1Atom", &payload) {
            Ok(result) => {
                self.state.logs.push(format!("Successfully created L1 Atom: {:?}", result));
                if let Some(atom_id) = result.get("result").and_then(|v| v.as_str()) {
                    match self.vm_client.get_atom(atom_id) {
                        Ok(atom_vm) => self.state.l1_atoms.push(atom_vm),
                        Err(e) => self.state.logs.push(format!("Failed to fetch L1 atom: {}", e)),
                    }
                }
            }
            Err(e) => {
                let error_msg = format!("Error creating L1 Atom: {}", e);
                error!("{}", error_msg);
                self.state.logs.push(error_msg);
            }
        }
        Ok(())
    }
}

fn next_l1_type(current: L1Type) -> L1Type {
    match current {
        L1Type::Observation => L1Type::Requirement,
        L1Type::Requirement => L1Type::Constraint,
        L1Type::Constraint => L1Type::Hypothesis,
        L1Type::Hypothesis => L1Type::Question,
        L1Type::Question => L1Type::Observation,
    }
}

fn classify_l1_type(text: &str) -> L1Type {
    let lowered = text.to_lowercase();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return L1Type::Question;
    }
    if trimmed.contains('?') || trimmed.contains('？')
        || lowered.starts_with("why")
        || lowered.starts_with("what")
        || lowered.starts_with("how")
        || trimmed.contains("なぜ")
        || trimmed.contains("どう")
        || trimmed.contains("何")
    {
        return L1Type::Question;
    }
    if lowered.contains("must") || lowered.contains("need") || lowered.contains("should")
        || trimmed.contains("必要") || trimmed.contains("べき") || trimmed.contains("要求")
    {
        return L1Type::Requirement;
    }
    if lowered.contains("cannot") || lowered.contains("must not") || lowered.contains("limit")
        || trimmed.contains("できない") || trimmed.contains("禁止") || trimmed.contains("制約") || trimmed.contains("上限") || trimmed.contains("下限")
    {
        return L1Type::Constraint;
    }
    if lowered.contains("maybe") || lowered.contains("might") || lowered.contains("hypothesis")
        || trimmed.contains("かもしれない") || trimmed.contains("仮説") || trimmed.contains("推測")
    {
        return L1Type::Hypothesis;
    }
    if trimmed.ends_with('.') || trimmed.ends_with("。") || trimmed.ends_with('!') || trimmed.ends_with("！") {
        return L1Type::Observation;
    }
    L1Type::Question
}
