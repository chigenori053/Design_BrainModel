use crate::apply::Changeset;

#[derive(Debug, Clone, Default)]
pub struct Transaction {
    changeset: Changeset,
    committed: bool,
}

impl Transaction {
    pub fn new(changeset: Changeset) -> Self {
        Self {
            changeset,
            committed: false,
        }
    }

    pub fn commit(&mut self) {
        self.committed = true;
    }

    pub fn rollback(&mut self) -> Changeset {
        self.committed = false;
        std::mem::take(&mut self.changeset)
    }

    pub fn is_committed(&self) -> bool {
        self.committed
    }

    pub fn changeset(&self) -> &Changeset {
        &self.changeset
    }
}
