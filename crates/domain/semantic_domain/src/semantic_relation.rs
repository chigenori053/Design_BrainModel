#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticRelation {
    pub from: u64,
    pub to: u64,
    pub label: String,
}
