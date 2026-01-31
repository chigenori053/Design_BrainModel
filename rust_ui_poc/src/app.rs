use anyhow::Result;
use log::{info, error};

use crate::model::{AppState, CreateL1AtomPayload, L1ClusterVm};
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
            Action::CreateL1Atom(content) => {
                self.state.logs.push(format!("Dispatching CreateL1Atom with content: {}", content));
                let payload = CreateL1AtomPayload {
                    content,
                    r#type: "ManualInput".to_string(),
                    source: "RustUI-Client".to_string(),
                };
                match self.vm_client.execute_command("CreateL1Atom", &payload) {
                    Ok(result) => {
                        self.state.logs.push(format!("Successfully created L1 Atom: {:?}", result));
                        // In a real app, we would now refresh the relevant view models.
                    }
                    Err(e) => {
                        let error_msg = format!("Error creating L1 Atom: {}", e);
                        error!("{}", error_msg);
                        self.state.logs.push(error_msg);
                    }
                }
            }
            Action::Tick => {
                // This action can be used for periodic updates, e.g., fetching data.
                // For now, we'll just log it.
                // info!("Tick received");
            }
        }
        Ok(())
    }
}
