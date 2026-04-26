use crate::ir_diff::IrChange;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeSet {
    pub changes: Vec<IrChange>,
}

impl ChangeSet {
    pub fn new(changes: Vec<IrChange>) -> Self {
        Self { changes }
    }

    pub fn push(&mut self, change: IrChange) {
        self.changes.push(change);
    }

    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}
