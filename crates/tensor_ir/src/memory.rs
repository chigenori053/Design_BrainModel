use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::entity::EntityId;
use crate::predicate::PredicateId;
use crate::relation::Relation;
use crate::rule::RuleId;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Experience {
    pub input_relations: Vec<Relation>,
    pub inferred_relations: Vec<Relation>,
    pub rules_applied: Vec<RuleId>,
    pub confidence: f32,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryConfig {
    pub min_confidence: f32,
    pub decay_lambda: f32,
    pub max_memory_size: usize,
    pub recall_top_k: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryController {
    pub config: MemoryConfig,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MemorySpace {
    pub experiences: Vec<Experience>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub relations: Vec<Relation>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecalledExperience {
    pub experience: Experience,
    pub score: f32,
}

impl MemorySpace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn append(&mut self, experience: Experience) {
        self.experiences.push(experience);
    }

    pub fn store(&mut self, mut exp: Experience, controller: &MemoryController, now: u64) {
        if !filter_confidence(&exp, &controller.config) {
            return;
        }

        apply_decay(&mut exp, now, controller.config.decay_lambda);
        if !filter_confidence(&exp, &controller.config) {
            return;
        }

        self.experiences.push(exp);
        apply_decay_to_all(&mut self.experiences, now, controller.config.decay_lambda);
        prune(&mut self.experiences, &controller.config);
    }

    pub fn recall(
        &self,
        query: &MemoryQuery,
        controller: &MemoryController,
        now: u64,
    ) -> Vec<RecalledExperience> {
        let mut recalled: Vec<RecalledExperience> = self
            .experiences
            .iter()
            .cloned()
            .filter_map(|experience| {
                let mut decayed = experience;
                apply_decay(&mut decayed, now, controller.config.decay_lambda);
                if !filter_confidence(&decayed, &controller.config) {
                    return None;
                }

                let overlap = similarity_score(&query.relations, &decayed.input_relations);
                let score = overlap * decayed.confidence;
                if score > 0.0 {
                    Some(RecalledExperience {
                        experience: decayed,
                        score,
                    })
                } else {
                    None
                }
            })
            .collect();

        recalled.sort_by(|lhs, rhs| {
            rhs.score
                .total_cmp(&lhs.score)
                .then_with(|| lhs.experience.timestamp.cmp(&rhs.experience.timestamp))
                .then_with(|| {
                    lhs.experience
                        .confidence
                        .total_cmp(&rhs.experience.confidence)
                })
        });
        recalled.truncate(controller.config.recall_top_k);

        recalled
            .into_iter()
            .map(|mut recalled| {
                recalled.experience.inferred_relations = filter_relations(
                    recalled.experience.inferred_relations,
                    controller.config.min_confidence,
                );
                recalled
            })
            .filter(|recalled| !recalled.experience.inferred_relations.is_empty())
            .collect()
    }
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            min_confidence: 0.1,
            decay_lambda: 0.0,
            max_memory_size: usize::MAX,
            recall_top_k: 3,
        }
    }
}

impl Default for MemoryController {
    fn default() -> Self {
        Self {
            config: MemoryConfig::default(),
        }
    }
}

pub fn similarity_score(query_relations: &[Relation], memory_relations: &[Relation]) -> f32 {
    if query_relations.is_empty() {
        return 0.0;
    }

    let query = relation_key_counts(query_relations);
    let memory = relation_key_counts(memory_relations);
    let overlap = query
        .iter()
        .map(|(key, query_count)| {
            let memory_count = memory.get(key).copied().unwrap_or(0);
            (*query_count).min(memory_count)
        })
        .sum::<usize>();

    overlap as f32 / query_relations.len() as f32
}

pub fn merge_relation_sets(left: Vec<Relation>, right: Vec<Relation>) -> Vec<Relation> {
    let mut merged = BTreeMap::<(EntityId, PredicateId, EntityId), Relation>::new();

    for relation in left.into_iter().chain(right.into_iter()) {
        let key = (relation.subject, relation.predicate, relation.object);
        merged
            .entry(key)
            .and_modify(|existing| {
                existing.weight = existing.weight.max(relation.weight);
            })
            .or_insert(relation);
    }

    merged.into_values().collect()
}

pub fn filter_relations(relations: Vec<Relation>, threshold: f32) -> Vec<Relation> {
    relations
        .into_iter()
        .filter(|relation| relation.weight >= threshold)
        .collect()
}

pub fn filter_confidence(exp: &Experience, cfg: &MemoryConfig) -> bool {
    exp.confidence >= cfg.min_confidence
}

pub fn apply_decay(exp: &mut Experience, now: u64, lambda: f32) {
    let dt = now.saturating_sub(exp.timestamp) as f32;
    let decay = (-lambda * dt).exp();
    exp.confidence *= decay;
}

pub fn prune(memory: &mut Vec<Experience>, cfg: &MemoryConfig) {
    memory.sort_by(|a, b| {
        b.confidence
            .total_cmp(&a.confidence)
            .then_with(|| a.timestamp.cmp(&b.timestamp))
    });
    memory.truncate(cfg.max_memory_size);
}

fn apply_decay_to_all(memory: &mut [Experience], now: u64, lambda: f32) {
    for experience in memory {
        apply_decay(experience, now, lambda);
    }
}

fn relation_key_counts(
    relations: &[Relation],
) -> BTreeMap<(EntityId, PredicateId, EntityId), usize> {
    let mut counts = BTreeMap::<(EntityId, PredicateId, EntityId), usize>::new();
    for relation in relations {
        *counts
            .entry((relation.subject, relation.predicate, relation.object))
            .or_default() += 1;
    }
    counts
}
