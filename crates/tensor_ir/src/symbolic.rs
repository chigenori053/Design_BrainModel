use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::entity::EntityId;
use crate::predicate::PredicateId;
use crate::relation::{Provenance, Relation};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolicRelation {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

impl SymbolicRelation {
    pub fn into_relation(
        self,
        entity_symbols: &BTreeMap<String, EntityId>,
        predicate_symbols: &BTreeMap<String, PredicateId>,
        weight: f32,
        provenance: Provenance,
    ) -> Option<Relation> {
        let subject = entity_symbols.get(&self.subject).copied()?;
        let predicate = predicate_symbols.get(&self.predicate).copied()?;
        let object = entity_symbols.get(&self.object).copied()?;
        Some(Relation::new(
            subject, predicate, object, weight, provenance,
        ))
    }
}

pub fn to_symbolic_relation(
    relation: &Relation,
    entity_labels: &BTreeMap<EntityId, String>,
    predicate_labels: &BTreeMap<PredicateId, String>,
) -> Option<SymbolicRelation> {
    Some(SymbolicRelation {
        subject: entity_labels.get(&relation.subject)?.clone(),
        predicate: predicate_labels.get(&relation.predicate)?.clone(),
        object: entity_labels.get(&relation.object)?.clone(),
    })
}
