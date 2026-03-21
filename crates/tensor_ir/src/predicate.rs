use serde::{Deserialize, Serialize};

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize,
)]
pub struct PredicateId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Predicate {
    pub id: PredicateId,
    pub name: String,
    pub arity: u8,
}
