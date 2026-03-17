use std::collections::BTreeSet;

use crate::stable_v03::{ArchitectureGraph, NodeId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    DuplicateNodeId(NodeId),
    MissingNode(NodeId),
    SelfLoop(NodeId),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValidationResult {
    pub errors: Vec<ValidationError>,
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

pub fn validate(graph: &ArchitectureGraph) -> ValidationResult {
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    for node in graph.nodes() {
        if !seen.insert(node.id.clone()) {
            errors.push(ValidationError::DuplicateNodeId(node.id.clone()));
        }
    }
    for edge in graph.edges() {
        if edge.source == edge.target {
            errors.push(ValidationError::SelfLoop(edge.source.clone()));
        }
        if !seen.contains(&edge.source) {
            errors.push(ValidationError::MissingNode(edge.source.clone()));
        }
        if !seen.contains(&edge.target) {
            errors.push(ValidationError::MissingNode(edge.target.clone()));
        }
    }
    ValidationResult { errors }
}
