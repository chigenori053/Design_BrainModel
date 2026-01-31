use crate::model::{CreateL1AtomPayload, L1AtomVm, L1ClusterVm, DecisionChipVm};
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::Value;
use log::{error, info};

const API_BASE_URL: &str = "http://localhost:8000";

/// A synchronous client to interact with the DesignBrainModel backend API.
#[derive(Debug, Clone)]
pub struct VmClient {
    base_url: String,
    client: Client,
}

impl VmClient {
    /// Creates a new client instance.
    pub fn new() -> Self {
        info!("Initializing VmClient for base URL: {}", API_BASE_URL);
        Self {
            base_url: API_BASE_URL.to_string(),
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(5))
                // Avoid macOS system proxy lookup panic in blocking client.
                .no_proxy()
                .build()
                .expect("Failed to build reqwest client"),
        }
    }

    // --- ViewModel Getters ---

    pub fn get_cluster(&self, cluster_id: &str) -> Result<L1ClusterVm, String> {
        let url = format!("{}/viewmodel/cluster/{}", self.base_url, cluster_id);
        info!("Fetching from URL: {}", url);
        
        self.client.get(&url)
            .send()
            .map_err(|e| {
                error!("Request failed for get_cluster({}): {}", cluster_id, e);
                e.to_string()
            })?
            .json::<L1ClusterVm>()
            .map_err(|e| {
                error!("Failed to parse JSON for get_cluster({}): {}", cluster_id, e);
                e.to_string()
            })
    }

    pub fn get_atom(&self, atom_id: &str) -> Result<L1AtomVm, String> {
        let url = format!("{}/viewmodel/atom/{}", self.base_url, atom_id);
        info!("Fetching from URL: {}", url);

        self.client.get(&url)
            .send()
            .map_err(|e| e.to_string())?
            .json::<L1AtomVm>()
            .map_err(|e| e.to_string())
    }

    pub fn get_decision(&self, decision_id: &str) -> Result<DecisionChipVm, String> {
        let url = format!("{}/viewmodel/decision/{}", self.base_url, decision_id);
        info!("Fetching from URL: {}", url);

        self.client.get(&url)
            .send()
            .map_err(|e| e.to_string())?
            .json::<DecisionChipVm>()
            .map_err(|e| e.to_string())
    }

    // --- Command Executor ---

    /// Executes a command on the backend.
    pub fn execute_command<T: Serialize>(&self, command_type: &str, payload: &T) -> Result<Value, String> {
        let url = format!("{}/command", self.base_url);
        let body = serde_json::json!({
            "command_type": command_type,
            "payload": payload
        });

        info!("Executing command '{}' with payload: {}", command_type, serde_json::to_string(payload).unwrap_or_default());

        match self.client.post(&url).json(&body).send() {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    resp.json::<Value>().map_err(|e| format!("Failed to parse successful response: {}", e))
                } else {
                    let text = resp.text().unwrap_or_else(|_| "Failed to read error body".to_string());
                    error!("API Error ({}): {}", status, text);
                    Err(format!("API Error ({}): {}", status, text))
                }
            },
            Err(e) => {
                error!("Failed to send command to server: {}", e);
                Err(e.to_string())
            }
        }
    }
}

impl Default for VmClient {
    fn default() -> Self {
        Self::new()
    }
}
