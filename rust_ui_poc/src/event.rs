use serde::{Serialize, Deserialize};
use crate::model::ConsensusStatus;


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UiEvent {
    UserInput(String),
    RequestReevaluation,
    HumanOverride {
        decision: ConsensusStatus,
        reason: String,
    },
}
