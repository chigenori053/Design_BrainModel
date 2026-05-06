use std::path::PathBuf;

use serde_json::json;

use super::transaction::{
    ApplyState, ExecutionTransaction, GitExecutionState, GitPhase, TransactionRecord,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitExecutionRecord {
    pub command: String,
    pub targets: Vec<PathBuf>,
    pub result: String,
    pub timestamp: u64,
}

pub fn git_record_json(record: &GitExecutionRecord) -> String {
    serde_json::to_string(&json!({
        "git_execution": {
            "command": record.command,
            "targets": record.targets.iter().map(|path| path.display().to_string()).collect::<Vec<_>>(),
            "result": record.result,
            "timestamp": record.timestamp,
        }
    }))
    .unwrap_or_else(|_| "{\"git_execution\":\"serialization_failed\"}".to_string())
}

pub fn transaction_record_json(transaction: &ExecutionTransaction) -> String {
    let record = TransactionRecord::from(transaction);
    serde_json::to_string(&json!({
        "transaction": {
            "transaction_id": record.transaction_id,
            "apply_state": apply_state_label(&record.apply_state),
            "git_state": git_state_json(&record.git_state),
            "started_at": record.started_at,
            "finalized_at": record.finalized_at,
        }
    }))
    .unwrap_or_else(|_| "{\"transaction\":\"serialization_failed\"}".to_string())
}

fn apply_state_label(state: &ApplyState) -> &'static str {
    match state {
        ApplyState::Pending => "Pending",
        ApplyState::Applied => "Applied",
        ApplyState::Failed => "Failed",
        ApplyState::RolledBack => "RolledBack",
    }
}

fn git_state_json(state: &GitExecutionState) -> serde_json::Value {
    match state {
        GitExecutionState::NotStarted => json!({ "type": "NotStarted" }),
        GitExecutionState::Staged { files } => json!({
            "type": "Staged",
            "files": files.iter().map(|path| path.display().to_string()).collect::<Vec<_>>()
        }),
        GitExecutionState::Committed { commit_hash } => json!({
            "type": "Committed",
            "commit_hash": commit_hash,
        }),
        GitExecutionState::Failed { phase, reason } => json!({
            "type": "Failed",
            "phase": git_phase_label(*phase),
            "reason": reason,
        }),
    }
}

fn git_phase_label(phase: GitPhase) -> &'static str {
    match phase {
        GitPhase::Add => "Add",
        GitPhase::Commit => "Commit",
    }
}
