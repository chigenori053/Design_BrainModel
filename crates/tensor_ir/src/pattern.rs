use serde::{Deserialize, Serialize};

use crate::predicate::PredicateId;

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Variable(pub String);

impl Variable {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationPattern {
    pub subject: Variable,
    pub predicate: PredicateId,
    pub object: Variable,
}
