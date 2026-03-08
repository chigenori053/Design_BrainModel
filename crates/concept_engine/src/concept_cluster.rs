use crate::concept::ConceptId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConceptCluster {
    pub domain: String,
    pub concepts: Vec<ConceptId>,
}
