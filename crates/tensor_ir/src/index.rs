use std::collections::HashMap;

use crate::entity::EntityId;
use crate::predicate::PredicateId;
use crate::relation::Relation;

#[derive(Clone, Debug, Default)]
pub struct RelationIndex {
    pub by_predicate: HashMap<PredicateId, Vec<Relation>>,
    pub by_subject: HashMap<EntityId, Vec<Relation>>,
    pub by_object: HashMap<EntityId, Vec<Relation>>,
}

impl RelationIndex {
    pub fn build(relations: &[Relation]) -> Self {
        let mut by_predicate = HashMap::<PredicateId, Vec<Relation>>::new();
        let mut by_subject = HashMap::<EntityId, Vec<Relation>>::new();
        let mut by_object = HashMap::<EntityId, Vec<Relation>>::new();

        for relation in relations {
            by_predicate
                .entry(relation.predicate)
                .or_default()
                .push(relation.clone());
            by_subject
                .entry(relation.subject)
                .or_default()
                .push(relation.clone());
            by_object
                .entry(relation.object)
                .or_default()
                .push(relation.clone());
        }

        for bucket in by_predicate.values_mut() {
            sort_relations(bucket);
        }
        for bucket in by_subject.values_mut() {
            sort_relations(bucket);
        }
        for bucket in by_object.values_mut() {
            sort_relations(bucket);
        }

        Self {
            by_predicate,
            by_subject,
            by_object,
        }
    }
}

fn sort_relations(relations: &mut [Relation]) {
    relations.sort_by(|lhs, rhs| {
        (lhs.subject, lhs.predicate, lhs.object)
            .cmp(&(rhs.subject, rhs.predicate, rhs.object))
            .then_with(|| lhs.weight.total_cmp(&rhs.weight))
    });
}
