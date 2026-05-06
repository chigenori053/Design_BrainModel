use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyState {
    Pending,
    Applied,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitExecutionState {
    NotStarted,
    Staged { files: Vec<PathBuf> },
    Committed { commit_hash: String },
    Failed { phase: GitPhase, reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitPhase {
    Add,
    Commit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionTransaction {
    pub transaction_id: String,
    pub apply_state: ApplyState,
    pub git_state: GitExecutionState,
    pub started_at: u64,
    pub finalized_at: Option<u64>,
}

impl ExecutionTransaction {
    pub fn new(transaction_id: String, started_at: u64) -> Self {
        Self {
            transaction_id,
            apply_state: ApplyState::Pending,
            git_state: GitExecutionState::NotStarted,
            started_at,
            finalized_at: None,
        }
    }

    pub fn mark_applied(&mut self) {
        self.apply_state = ApplyState::Applied;
    }

    pub fn mark_apply_failed(&mut self, reason: impl Into<String>) {
        self.apply_state = ApplyState::Failed;
        self.git_state = GitExecutionState::Failed {
            phase: GitPhase::Add,
            reason: format!("git prohibited after apply failure: {}", reason.into()),
        };
    }

    pub fn mark_staged(&mut self, files: Vec<PathBuf>) {
        self.git_state = GitExecutionState::Staged { files };
    }

    pub fn mark_git_failed(&mut self, phase: GitPhase, reason: impl Into<String>) {
        self.git_state = GitExecutionState::Failed {
            phase,
            reason: reason.into(),
        };
    }

    pub fn mark_committed(&mut self, commit_hash: String) {
        self.git_state = GitExecutionState::Committed { commit_hash };
    }

    pub fn finalize(&mut self, timestamp: u64) {
        self.finalized_at = Some(timestamp);
    }

    pub fn fail_safe_finalize_after_recovery(&mut self, timestamp: u64) {
        if self.finalized_at.is_none() {
            self.finalized_at = Some(timestamp);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransactionRecord {
    pub transaction_id: String,
    pub apply_state: ApplyState,
    pub git_state: GitExecutionState,
    pub started_at: u64,
    pub finalized_at: Option<u64>,
}

impl From<&ExecutionTransaction> for TransactionRecord {
    fn from(transaction: &ExecutionTransaction) -> Self {
        Self {
            transaction_id: transaction.transaction_id.clone(),
            apply_state: transaction.apply_state.clone(),
            git_state: transaction.git_state.clone(),
            started_at: transaction.started_at,
            finalized_at: transaction.finalized_at,
        }
    }
}
