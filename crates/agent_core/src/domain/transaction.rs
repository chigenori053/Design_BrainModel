use crate::domain::history::SessionSnapshot;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransactionEngine {
    pub(crate) current_tx: Option<ActiveTransaction>,
}

impl TransactionEngine {
    pub fn new() -> Self {
        Self { current_tx: None }
    }

    pub fn current_tx(&self) -> Option<&ActiveTransaction> {
        self.current_tx.as_ref()
    }

    pub fn has_active_tx(&self) -> bool {
        self.current_tx.is_some()
    }
}

impl Default for TransactionEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveTransaction {
    pub snapshot_before: SessionSnapshot,
    pub diffs: Vec<ProposedDiff>,
    pub status: TxStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TxStatus {
    Pending,
    Validated,
    Applied,
    Committed,
    Aborted,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposedDiff {
    UpsertNode {
        key: String,
        value: String,
    },
    RemoveNode {
        key: String,
    },
    SetDependencies {
        key: String,
        dependencies: Vec<String>,
    },
    RemoveDependencies {
        key: String,
    },
    SplitHighOutDegreeNode {
        key: String,
    },
    RewireHighImpactEdge {
        key: String,
        from: String,
        to: String,
    },
    TwoStep {
        first: Box<ProposedDiff>,
        second: Box<ProposedDiff>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TxError {
    ActiveTransactionExists,
    NoActiveTransaction,
    InvalidTransactionState {
        expected: TxStatus,
        actual: TxStatus,
    },
    MissingNode(String),
    MissingDependency(String),
    InvalidSplitCandidate(String),
    InvalidRewireCandidate(String),
    CycleIncreaseRejected(String),
    TransactionInProgress,
    UndoUnavailable,
    RedoUnavailable,
}
