use std::collections::BTreeMap;

use crate::entity::EntityId;
use crate::index::RelationIndex;
use crate::memory::{Experience, MemoryController, MemoryQuery, MemorySpace, merge_relation_sets};
use crate::predicate::PredicateId;
use crate::relation::Relation;
use crate::rule::{Rule, RuleId};

#[derive(Clone, Debug, PartialEq)]
pub struct TensorLogicEngine {
    pub max_steps: usize,
    pub threshold: f32,
}

impl TensorLogicEngine {
    pub fn infer(&self, relations: Vec<Relation>, rules: &[Rule]) -> Vec<Relation> {
        let mut current = self.sort_relations(relations);
        let mut step = 0;

        loop {
            let previous_len = current.len();
            let next = self.forward_step(&current, rules);
            let merged = self.merge(current, next);

            if self.is_fixpoint(previous_len, merged.len()) || step >= self.max_steps {
                return merged;
            }

            current = merged;
            step += 1;
        }
    }

    pub fn infer_with_memory(
        &self,
        input: Vec<Relation>,
        rules: &[Rule],
        memory: &mut MemorySpace,
        controller: &MemoryController,
        timestamp: u64,
    ) -> Vec<Relation> {
        let recalled = memory.recall(
            &MemoryQuery {
                relations: input.clone(),
            },
            controller,
            timestamp,
        );
        let recalled_relations = recalled
            .into_iter()
            .flat_map(|recalled| recalled.experience.inferred_relations)
            .collect::<Vec<_>>();
        let initial = merge_relation_sets(input.clone(), recalled_relations);
        let inferred = self.infer(initial, rules);
        let inferred_only = inferred
            .iter()
            .filter(|relation| !contains_relation(&input, relation))
            .cloned()
            .collect::<Vec<_>>();

        memory.store(
            Experience {
                input_relations: self.sort_relations(input),
                inferred_relations: self.sort_relations(inferred_only.clone()),
                rules_applied: self.collect_applied_rule_ids(rules, &inferred_only),
                confidence: confidence(&inferred_only),
                timestamp,
            },
            controller,
            timestamp,
        );

        inferred
    }

    fn forward_step(&self, relations: &[Relation], rules: &[Rule]) -> Vec<Relation> {
        let mut new_relations = Vec::new();
        let index = RelationIndex::build(relations);

        for rule in rules {
            for relation in rule.apply_indexed(&index) {
                if relation.weight >= self.threshold {
                    new_relations.push(relation);
                }
            }
        }

        self.sort_relations(new_relations)
    }

    fn merge(&self, current: Vec<Relation>, new: Vec<Relation>) -> Vec<Relation> {
        let mut map = BTreeMap::<(EntityId, PredicateId, EntityId), Relation>::new();

        for relation in current.into_iter().chain(new.into_iter()) {
            let key = (relation.subject, relation.predicate, relation.object);
            map.entry(key)
                .and_modify(|existing| {
                    existing.weight = existing.weight.max(relation.weight);
                })
                .or_insert(relation);
        }

        map.into_values().collect()
    }

    fn is_fixpoint(&self, prev_len: usize, current_len: usize) -> bool {
        prev_len == current_len
    }

    fn sort_relations(&self, mut relations: Vec<Relation>) -> Vec<Relation> {
        relations.sort_by(|lhs, rhs| {
            (lhs.subject, lhs.predicate, lhs.object)
                .cmp(&(rhs.subject, rhs.predicate, rhs.object))
                .then_with(|| lhs.weight.total_cmp(&rhs.weight))
        });
        relations
    }

    fn collect_applied_rule_ids(
        &self,
        rules: &[Rule],
        inferred_relations: &[Relation],
    ) -> Vec<RuleId> {
        rules
            .iter()
            .enumerate()
            .filter_map(|(index, rule)| {
                inferred_relations
                    .iter()
                    .any(|relation| relation.predicate == rule.head.predicate)
                    .then_some(RuleId(index as u64))
            })
            .collect()
    }
}

fn contains_relation(relations: &[Relation], target: &Relation) -> bool {
    relations.iter().any(|relation| {
        relation.subject == target.subject
            && relation.predicate == target.predicate
            && relation.object == target.object
    })
}

fn confidence(relations: &[Relation]) -> f32 {
    if relations.is_empty() {
        return 0.0;
    }
    relations
        .iter()
        .map(|relation| relation.weight)
        .sum::<f32>()
        / relations.len() as f32
}
