#[derive(Debug, Clone, Default)]
pub struct Changeset {
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone)]
pub struct Change {
    pub target: String,
    pub kind: ChangeKind,
}

#[derive(Debug, Clone)]
pub enum ChangeKind {
    Insert(String),
    Delete,
    Replace(String),
}

#[derive(Debug, Clone, Default)]
pub struct ApplyResult {
    pub applied: usize,
    pub skipped: usize,
}

pub trait ApplyEngine {
    fn apply(&mut self, changeset: &Changeset) -> ApplyResult;
}
