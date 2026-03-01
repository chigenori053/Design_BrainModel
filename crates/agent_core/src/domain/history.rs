use crate::domain::state::{DesignScoreVector, UnifiedDesignState};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionSnapshot {
    pub version_id: u64,
    pub uds_hash: u64,
    pub uds: UnifiedDesignState,
    pub evaluation: DesignScoreVector,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionHistory {
    snapshots: Vec<SessionSnapshot>,
    current_index: usize,
    max_size: usize,
}

impl SessionHistory {
    pub fn new(max_size: usize) -> Self {
        let max_size = max_size.max(1);
        Self {
            snapshots: Vec::new(),
            current_index: 0,
            max_size,
        }
    }

    pub fn with_initial(snapshot: SessionSnapshot, max_size: usize) -> Self {
        let mut history = Self::new(max_size);
        history.snapshots.push(snapshot);
        history.current_index = 0;
        history
    }

    pub fn push(&mut self, snapshot: SessionSnapshot) {
        if !self.snapshots.is_empty() && self.current_index + 1 < self.snapshots.len() {
            self.snapshots.truncate(self.current_index + 1);
        }

        self.snapshots.push(snapshot);

        while self.snapshots.len() > self.max_size {
            self.snapshots.remove(0);
        }

        self.current_index = self.snapshots.len().saturating_sub(1);
    }

    pub fn undo(&mut self) -> Option<SessionSnapshot> {
        if self.current_index == 0 || self.snapshots.is_empty() {
            return None;
        }
        self.current_index -= 1;
        self.snapshots.get(self.current_index).cloned()
    }

    pub fn redo(&mut self) -> Option<SessionSnapshot> {
        if self.snapshots.is_empty() || self.current_index + 1 >= self.snapshots.len() {
            return None;
        }
        self.current_index += 1;
        self.snapshots.get(self.current_index).cloned()
    }

    pub fn current(&self) -> Option<&SessionSnapshot> {
        self.snapshots.get(self.current_index)
    }

    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn current_index(&self) -> usize {
        self.current_index
    }

    pub fn max_size(&self) -> usize {
        self.max_size
    }
}
