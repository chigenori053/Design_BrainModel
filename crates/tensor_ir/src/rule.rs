use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::entity::EntityId;
use crate::index::RelationIndex;
use crate::pattern::{RelationPattern, Variable};
use crate::predicate::PredicateId;
use crate::relation::{Provenance, Relation};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub head: RelationPattern,
    pub body: Vec<RelationPattern>,
    pub confidence: f32,
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize,
)]
pub struct RuleId(pub u64);

impl Rule {
    pub fn apply(&self, relations: &[Relation]) -> Vec<Relation> {
        self.apply_scan(relations)
    }

    pub fn apply_indexed(&self, index: &RelationIndex) -> Vec<Relation> {
        if self.body.is_empty() {
            return Vec::new();
        }

        let ordered_patterns = self.ordered_patterns(index);
        let mut states = Vec::new();
        self.expand_patterns(
            &ordered_patterns,
            0,
            Binding::default(),
            1.0,
            index,
            &mut states,
        );
        self.build_head_relations(states)
    }

    pub fn apply_scan(&self, relations: &[Relation]) -> Vec<Relation> {
        if self.body.is_empty() {
            return Vec::new();
        }

        let mut bindings = vec![BindingState::default()];
        for pattern in &self.body {
            let mut next = Vec::new();
            for binding in &bindings {
                for relation in relations
                    .iter()
                    .filter(|relation| relation.predicate == pattern.predicate)
                {
                    if let Some(bound) = unify_pattern(pattern, relation, binding) {
                        next.push(bound);
                    }
                }
            }
            bindings = dedup_binding_states(next);
            if bindings.is_empty() {
                break;
            }
        }

        self.build_head_relations(bindings)
    }

    fn ordered_patterns(&self, index: &RelationIndex) -> Vec<&RelationPattern> {
        let mut patterns: Vec<&RelationPattern> = self.body.iter().collect();
        patterns.sort_by_key(|pattern| {
            index
                .by_predicate
                .get(&pattern.predicate)
                .map(Vec::len)
                .unwrap_or(0)
        });
        patterns
    }

    fn expand_patterns(
        &self,
        patterns: &[&RelationPattern],
        depth: usize,
        binding: Binding,
        weight: f32,
        index: &RelationIndex,
        out: &mut Vec<BindingState>,
    ) {
        if depth == patterns.len() {
            out.push(BindingState { binding, weight });
            return;
        }

        let pattern = patterns[depth];
        for relation in candidate_relations(pattern, &binding, index) {
            if let Some(next_binding) = unify_binding(pattern, relation, &binding) {
                let next_weight = propagate_weight(weight, relation.weight, 1.0);
                self.expand_patterns(patterns, depth + 1, next_binding, next_weight, index, out);
            }
        }
    }

    fn build_head_relations(&self, states: Vec<BindingState>) -> Vec<Relation> {
        let mut deduped = BTreeMap::<(EntityId, PredicateId, EntityId), Relation>::new();

        for state in states {
            let Some(subject) = state.binding.map.get(&self.head.subject).copied() else {
                continue;
            };
            let Some(object) = state.binding.map.get(&self.head.object).copied() else {
                continue;
            };

            let relation = Relation::new(
                subject,
                self.head.predicate,
                object,
                propagate_weight(state.weight, 1.0, self.confidence),
                Provenance::Inferred,
            );
            let key = (relation.subject, relation.predicate, relation.object);
            deduped
                .entry(key)
                .and_modify(|existing| existing.weight = existing.weight.max(relation.weight))
                .or_insert(relation);
        }

        deduped.into_values().collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Binding {
    pub map: BTreeMap<Variable, EntityId>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct BindingState {
    binding: Binding,
    weight: f32,
}

fn unify_pattern(
    pattern: &RelationPattern,
    relation: &Relation,
    current: &BindingState,
) -> Option<BindingState> {
    let next_binding = unify_binding(pattern, relation, &current.binding)?;
    let weight = if current.weight == 0.0 {
        relation.weight
    } else {
        propagate_weight(current.weight, relation.weight, 1.0)
    };
    Some(BindingState {
        binding: next_binding,
        weight,
    })
}

fn unify_binding(
    pattern: &RelationPattern,
    relation: &Relation,
    current: &Binding,
) -> Option<Binding> {
    let mut next = current.clone();
    unify_variable(&pattern.subject, relation.subject, &mut next.map)?;
    unify_variable(&pattern.object, relation.object, &mut next.map)?;
    Some(next)
}

fn unify_variable(
    variable: &Variable,
    entity: EntityId,
    bindings: &mut BTreeMap<Variable, EntityId>,
) -> Option<()> {
    match bindings.get(variable) {
        Some(bound) if *bound != entity => None,
        Some(_) => Some(()),
        None => {
            bindings.insert(variable.clone(), entity);
            Some(())
        }
    }
}

fn dedup_binding_states(bindings: Vec<BindingState>) -> Vec<BindingState> {
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for binding in bindings {
        let signature = binding_signature(&binding);
        if seen.insert(signature) {
            unique.push(binding);
        }
    }
    unique
}

fn binding_signature(binding: &BindingState) -> Vec<(Variable, EntityId)> {
    binding
        .binding
        .map
        .iter()
        .map(|(variable, entity)| (variable.clone(), *entity))
        .collect()
}

fn candidate_relations<'a>(
    pattern: &RelationPattern,
    binding: &Binding,
    index: &'a RelationIndex,
) -> Vec<&'a Relation> {
    let subject = binding.map.get(&pattern.subject).copied();
    let object = binding.map.get(&pattern.object).copied();

    let candidates: Vec<&Relation> = match (subject, object) {
        (Some(subject), Some(object)) => index
            .by_subject
            .get(&subject)
            .into_iter()
            .flat_map(|relations| relations.iter())
            .filter(|relation| relation.predicate == pattern.predicate && relation.object == object)
            .collect(),
        (Some(subject), None) => index
            .by_subject
            .get(&subject)
            .into_iter()
            .flat_map(|relations| relations.iter())
            .filter(|relation| relation.predicate == pattern.predicate)
            .collect(),
        (None, Some(object)) => index
            .by_object
            .get(&object)
            .into_iter()
            .flat_map(|relations| relations.iter())
            .filter(|relation| relation.predicate == pattern.predicate)
            .collect(),
        (None, None) => index
            .by_predicate
            .get(&pattern.predicate)
            .into_iter()
            .flat_map(|relations| relations.iter())
            .collect(),
    };

    let mut candidates = candidates;
    candidates.sort_by(|lhs, rhs| {
        (lhs.subject, lhs.predicate, lhs.object)
            .cmp(&(rhs.subject, rhs.predicate, rhs.object))
            .then_with(|| lhs.weight.total_cmp(&rhs.weight))
    });
    candidates
}

pub fn propagate_weight(w1: f32, w2: f32, rule_conf: f32) -> f32 {
    w1 * w2 * rule_conf
}
