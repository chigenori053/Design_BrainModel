use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConsensusStatus {
    Pending,
    Reached,
    Failed,
    Reevaluating,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConfidenceLevel {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EntropyLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionDto {
    pub id: String,
    pub status: ConsensusStatus,
    pub selected_candidate: Option<String>,
    pub evaluator_count: usize,
    pub confidence: ConfidenceLevel,
    pub entropy: EntropyLevel,
    pub explanation: String,
    pub human_override: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionSummaryDto {
    pub id: String,
    pub status: ConsensusStatus,
    pub is_reevaluation: bool,
}

#[derive(Debug, Clone)]
pub struct UiState {
    pub latest_decision: Option<DecisionDto>,
    pub decision_history: Vec<DecisionSummaryDto>,
    pub input_buffer: String,
}

impl UiState {
    pub fn new() -> Self {
        Self {
            latest_decision: None,
            decision_history: Vec::new(),
            input_buffer: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decision_dto_serialization() {
        let dto = DecisionDto {
            id: "test-id".to_string(),
            status: ConsensusStatus::Reached,
            selected_candidate: Some("A".to_string()),
            evaluator_count: 3,
            confidence: ConfidenceLevel::High,
            entropy: EntropyLevel::Low,
            explanation: "test".to_string(),
            human_override: false,
        };

        let json = serde_json::to_string(&dto).expect("Failed to serialize");
        assert!(json.contains("test-id"));
        assert!(json.contains("Reached"));
    }
}
