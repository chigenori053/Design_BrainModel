use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::entity::EntityId;
use crate::predicate::PredicateId;
use crate::relation::{Provenance, Relation};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tensor3 {
    pub entity_count: usize,
    pub data: BTreeMap<PredicateId, Vec<Vec<f32>>>,
}

impl Tensor3 {
    pub fn from_relations(relations: &[Relation]) -> Self {
        let entity_ids = collect_entity_ids(relations);
        let entity_count = entity_ids
            .iter()
            .max()
            .map(|id| id.0 as usize + 1)
            .unwrap_or(0);
        let mut data = BTreeMap::<PredicateId, Vec<Vec<f32>>>::new();

        for relation in relations {
            let plane = data
                .entry(relation.predicate)
                .or_insert_with(|| vec![vec![0.0; entity_count]; entity_count]);
            plane[relation.subject.0 as usize][relation.object.0 as usize] = relation.weight;
        }

        Self { entity_count, data }
    }

    pub fn weight(&self, predicate: PredicateId, subject: EntityId, object: EntityId) -> f32 {
        self.data
            .get(&predicate)
            .and_then(|plane| plane.get(subject.0 as usize))
            .and_then(|row| row.get(object.0 as usize))
            .copied()
            .unwrap_or(0.0)
    }

    pub fn compose_predicate(&self, predicate: PredicateId) -> Vec<Relation> {
        let Some(plane) = self.data.get(&predicate) else {
            return Vec::new();
        };

        let mut composed = Vec::new();
        for subject in 0..self.entity_count {
            let subject_row = &plane[subject];
            for object in 0..self.entity_count {
                let mut weight = 0.0;
                for (pivot, pivot_row) in plane.iter().take(self.entity_count).enumerate() {
                    weight += subject_row[pivot] * pivot_row[object];
                }
                if weight > 0.0 {
                    composed.push(Relation::new(
                        EntityId(subject as u64),
                        predicate,
                        EntityId(object as u64),
                        weight.min(1.0),
                        Provenance::Inferred,
                    ));
                }
            }
        }
        composed
    }
}

fn collect_entity_ids(relations: &[Relation]) -> BTreeSet<EntityId> {
    relations
        .iter()
        .flat_map(|relation| [relation.subject, relation.object])
        .collect()
}
