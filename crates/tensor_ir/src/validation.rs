use std::collections::{BTreeMap, BTreeSet};

use crate::entity::{Entity, EntityId, EntityType};
use crate::predicate::{Predicate, PredicateId};
use crate::relation::Relation;
use crate::rule::Rule;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValidationError {
    pub message: String,
}

#[derive(Clone, Debug, Default)]
pub struct ValidationContext {
    pub entities: BTreeMap<EntityId, Entity>,
    pub predicates: BTreeMap<PredicateId, Predicate>,
    pub allowed_types: BTreeMap<PredicateId, (BTreeSet<EntityType>, BTreeSet<EntityType>)>,
    pub max_self_loops_per_predicate: usize,
}

impl ValidationContext {
    pub fn validate_relations(&self, relations: &[Relation]) -> Result<(), ValidationError> {
        let mut self_loops = BTreeMap::<PredicateId, usize>::new();

        for relation in relations {
            if !self.entities.contains_key(&relation.subject) {
                return Err(ValidationError {
                    message: format!("missing subject entity {}", relation.subject.0),
                });
            }
            if !self.entities.contains_key(&relation.object) {
                return Err(ValidationError {
                    message: format!("missing object entity {}", relation.object.0),
                });
            }

            let Some(predicate) = self.predicates.get(&relation.predicate) else {
                return Err(ValidationError {
                    message: format!("missing predicate {}", relation.predicate.0),
                });
            };
            if predicate.arity != 2 {
                return Err(ValidationError {
                    message: format!("predicate {} must have arity 2", predicate.name),
                });
            }

            if let Some((subject_types, object_types)) = self.allowed_types.get(&relation.predicate)
            {
                let subject = &self.entities[&relation.subject];
                let object = &self.entities[&relation.object];
                if !subject_types.is_empty() && !subject_types.contains(&subject.type_tag) {
                    return Err(ValidationError {
                        message: format!("invalid subject type for predicate {}", predicate.name),
                    });
                }
                if !object_types.is_empty() && !object_types.contains(&object.type_tag) {
                    return Err(ValidationError {
                        message: format!("invalid object type for predicate {}", predicate.name),
                    });
                }
            }

            if relation.subject == relation.object {
                let count = self_loops.entry(relation.predicate).or_default();
                *count += 1;
                if self.max_self_loops_per_predicate > 0
                    && *count > self.max_self_loops_per_predicate
                {
                    return Err(ValidationError {
                        message: format!(
                            "self-loop limit exceeded for predicate {}",
                            predicate.name
                        ),
                    });
                }
            }
        }

        Ok(())
    }

    pub fn validate_rule(&self, rule: &Rule) -> Result<(), ValidationError> {
        if !self.predicates.contains_key(&rule.head.predicate) {
            return Err(ValidationError {
                message: format!("missing head predicate {}", rule.head.predicate.0),
            });
        }

        for pattern in &rule.body {
            if !self.predicates.contains_key(&pattern.predicate) {
                return Err(ValidationError {
                    message: format!("missing body predicate {}", pattern.predicate.0),
                });
            }
        }

        Ok(())
    }
}
