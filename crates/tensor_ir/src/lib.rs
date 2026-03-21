pub mod engine;
pub mod entity;
pub mod index;
pub mod memory;
pub mod ops;
pub mod pattern;
pub mod predicate;
pub mod relation;
pub mod rule;
pub mod symbolic;
pub mod tensor;
pub mod validation;

pub use engine::TensorLogicEngine;
pub use entity::{Entity, EntityId, EntityType};
pub use index::RelationIndex;
pub use memory::{
    Experience, MemoryConfig, MemoryController, MemoryQuery, MemorySpace, RecalledExperience,
};
pub use ops::compose::compose;
pub use ops::filter::filter;
pub use ops::join::join;
pub use pattern::{RelationPattern, Variable};
pub use predicate::{Predicate, PredicateId};
pub use relation::{Provenance, Relation};
pub use rule::{Binding, Rule, RuleId};
pub use symbolic::{SymbolicRelation, to_symbolic_relation};
pub use tensor::Tensor3;
pub use validation::{ValidationContext, ValidationError};

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    #[test]
    fn join_and_compose_work_for_dependency_chain() {
        let dep = PredicateId(1);
        let ab = Relation::new(EntityId(1), dep, EntityId(2), 0.8, Provenance::Memory);
        let bc = Relation::new(EntityId(2), dep, EntityId(3), 0.7, Provenance::Memory);

        let joined = join(std::slice::from_ref(&ab), std::slice::from_ref(&bc));
        assert_eq!(joined.len(), 1);
        assert_eq!(joined[0].subject, EntityId(1));
        assert_eq!(joined[0].object, EntityId(3));
        assert!((joined[0].weight - 0.56).abs() < 1e-6);

        let composed = compose(&ab, &bc).expect("composable relation");
        assert_eq!(composed, joined[0]);
    }

    #[test]
    fn filter_and_rule_application_are_deterministic() {
        let dep = PredicateId(1);
        let rule = Rule {
            head: RelationPattern {
                subject: Variable::new("A"),
                predicate: dep,
                object: Variable::new("C"),
            },
            body: vec![
                RelationPattern {
                    subject: Variable::new("A"),
                    predicate: dep,
                    object: Variable::new("B"),
                },
                RelationPattern {
                    subject: Variable::new("B"),
                    predicate: dep,
                    object: Variable::new("C"),
                },
            ],
            confidence: 0.9,
        };

        let relations = vec![
            Relation::new(EntityId(1), dep, EntityId(2), 0.8, Provenance::Memory),
            Relation::new(EntityId(2), dep, EntityId(3), 0.7, Provenance::Memory),
            Relation::new(EntityId(4), dep, EntityId(5), 0.2, Provenance::Memory),
        ];

        let filtered = filter(&relations, 0.5);
        assert_eq!(filtered.len(), 2);

        let inferred = rule.apply(&filtered);
        assert_eq!(inferred.len(), 1);
        assert_eq!(inferred[0].subject, EntityId(1));
        assert_eq!(inferred[0].object, EntityId(3));
        assert_eq!(inferred[0].predicate, dep);
        assert_eq!(inferred[0].provenance, Provenance::Inferred);
        assert!((inferred[0].weight - 0.504).abs() < 1e-6);
    }

    #[test]
    fn tensor_conversion_and_contraction_work() {
        let dep = PredicateId(1);
        let relations = vec![
            Relation::new(EntityId(0), dep, EntityId(1), 0.5, Provenance::Memory),
            Relation::new(EntityId(1), dep, EntityId(2), 0.4, Provenance::Memory),
        ];

        let tensor = Tensor3::from_relations(&relations);
        assert_eq!(tensor.weight(dep, EntityId(0), EntityId(1)), 0.5);

        let composed = tensor.compose_predicate(dep);
        let inferred = composed
            .into_iter()
            .find(|relation| relation.subject == EntityId(0) && relation.object == EntityId(2))
            .expect("contraction result");
        assert!((inferred.weight - 0.2).abs() < 1e-6);
    }

    #[test]
    fn validation_and_memory_round_trip_hold_structure() {
        let api = Entity {
            id: EntityId(1),
            type_tag: EntityType::Object,
        };
        let auth = Entity {
            id: EntityId(2),
            type_tag: EntityType::Object,
        };
        let requires = Predicate {
            id: PredicateId(1),
            name: "requires".to_string(),
            arity: 2,
        };
        let relation = Relation::new(api.id, requires.id, auth.id, 0.95, Provenance::LLMGenerated);

        let mut allowed_subject_types = BTreeSet::new();
        allowed_subject_types.insert(EntityType::Object);
        let mut allowed_object_types = BTreeSet::new();
        allowed_object_types.insert(EntityType::Object);

        let mut context = ValidationContext::default();
        context.entities.insert(api.id, api.clone());
        context.entities.insert(auth.id, auth.clone());
        context.predicates.insert(requires.id, requires.clone());
        context
            .allowed_types
            .insert(requires.id, (allowed_subject_types, allowed_object_types));
        context.max_self_loops_per_predicate = 1;

        context
            .validate_relations(std::slice::from_ref(&relation))
            .expect("valid relation");

        let experience = Experience {
            input_relations: vec![relation.clone()],
            inferred_relations: vec![relation.clone()],
            rules_applied: vec![RuleId(0)],
            confidence: relation.score(1.0, 1.0),
            timestamp: 42,
        };
        let encoded = serde_json::to_string(&experience).expect("serialize");
        let decoded: Experience = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded, experience);

        let mut entity_labels = BTreeMap::new();
        entity_labels.insert(api.id, "API".to_string());
        entity_labels.insert(auth.id, "Auth".to_string());
        let mut predicate_labels = BTreeMap::new();
        predicate_labels.insert(requires.id, "requires".to_string());

        let symbolic = to_symbolic_relation(&relation, &entity_labels, &predicate_labels)
            .expect("symbolic relation");
        assert_eq!(symbolic.subject, "API");
        assert_eq!(symbolic.object, "Auth");
    }

    #[test]
    fn engine_infers_transitive_relations() {
        let dep = PredicateId(1);
        let engine = TensorLogicEngine {
            max_steps: 4,
            threshold: 0.1,
        };
        let rule = Rule {
            head: RelationPattern {
                subject: Variable::new("A"),
                predicate: dep,
                object: Variable::new("C"),
            },
            body: vec![
                RelationPattern {
                    subject: Variable::new("A"),
                    predicate: dep,
                    object: Variable::new("B"),
                },
                RelationPattern {
                    subject: Variable::new("B"),
                    predicate: dep,
                    object: Variable::new("C"),
                },
            ],
            confidence: 0.9,
        };

        let inferred = engine.infer(
            vec![
                Relation::new(EntityId(1), dep, EntityId(2), 0.8, Provenance::Memory),
                Relation::new(EntityId(2), dep, EntityId(3), 0.7, Provenance::Memory),
            ],
            &[rule],
        );

        assert!(
            inferred
                .iter()
                .any(|relation| relation.subject == EntityId(1) && relation.object == EntityId(3))
        );
    }

    #[test]
    fn engine_merges_duplicates_into_one_relation() {
        let dep = PredicateId(1);
        let engine = TensorLogicEngine {
            max_steps: 1,
            threshold: 0.1,
        };

        let merged = engine.infer(
            vec![
                Relation::new(EntityId(1), dep, EntityId(2), 0.4, Provenance::Memory),
                Relation::new(EntityId(1), dep, EntityId(2), 0.8, Provenance::Inferred),
            ],
            &[],
        );

        assert_eq!(merged.len(), 1);
        assert!((merged[0].weight - 0.8).abs() < 1e-6);
    }

    #[test]
    fn engine_applies_threshold() {
        let dep = PredicateId(1);
        let engine = TensorLogicEngine {
            max_steps: 2,
            threshold: 0.6,
        };
        let rule = Rule {
            head: RelationPattern {
                subject: Variable::new("A"),
                predicate: dep,
                object: Variable::new("C"),
            },
            body: vec![
                RelationPattern {
                    subject: Variable::new("A"),
                    predicate: dep,
                    object: Variable::new("B"),
                },
                RelationPattern {
                    subject: Variable::new("B"),
                    predicate: dep,
                    object: Variable::new("C"),
                },
            ],
            confidence: 0.9,
        };

        let inferred = engine.infer(
            vec![
                Relation::new(EntityId(1), dep, EntityId(2), 0.8, Provenance::Memory),
                Relation::new(EntityId(2), dep, EntityId(3), 0.7, Provenance::Memory),
            ],
            &[rule],
        );

        assert!(
            !inferred
                .iter()
                .any(|relation| relation.subject == EntityId(1) && relation.object == EntityId(3))
        );
    }

    #[test]
    fn engine_converges_within_max_steps() {
        let dep = PredicateId(1);
        let engine = TensorLogicEngine {
            max_steps: 1,
            threshold: 0.1,
        };
        let rule = Rule {
            head: RelationPattern {
                subject: Variable::new("A"),
                predicate: dep,
                object: Variable::new("C"),
            },
            body: vec![
                RelationPattern {
                    subject: Variable::new("A"),
                    predicate: dep,
                    object: Variable::new("B"),
                },
                RelationPattern {
                    subject: Variable::new("B"),
                    predicate: dep,
                    object: Variable::new("C"),
                },
            ],
            confidence: 0.9,
        };

        let inferred = engine.infer(
            vec![
                Relation::new(EntityId(1), dep, EntityId(2), 0.9, Provenance::Memory),
                Relation::new(EntityId(2), dep, EntityId(3), 0.9, Provenance::Memory),
                Relation::new(EntityId(3), dep, EntityId(4), 0.9, Provenance::Memory),
            ],
            &[rule],
        );

        assert!(inferred.len() <= 6);
    }

    #[test]
    fn relation_index_builds_lookup_tables() {
        let dep = PredicateId(1);
        let relations = vec![
            Relation::new(EntityId(1), dep, EntityId(2), 0.8, Provenance::Memory),
            Relation::new(EntityId(1), dep, EntityId(3), 0.7, Provenance::Memory),
        ];

        let index = RelationIndex::build(&relations);

        assert_eq!(index.by_predicate.get(&dep).map(Vec::len), Some(2));
        assert_eq!(index.by_subject.get(&EntityId(1)).map(Vec::len), Some(2));
        assert_eq!(index.by_object.get(&EntityId(2)).map(Vec::len), Some(1));
    }

    #[test]
    fn engine_supports_multi_hop_inference() {
        let dep = PredicateId(1);
        let engine = TensorLogicEngine {
            max_steps: 4,
            threshold: 0.1,
        };
        let rule = Rule {
            head: RelationPattern {
                subject: Variable::new("A"),
                predicate: dep,
                object: Variable::new("C"),
            },
            body: vec![
                RelationPattern {
                    subject: Variable::new("A"),
                    predicate: dep,
                    object: Variable::new("B"),
                },
                RelationPattern {
                    subject: Variable::new("B"),
                    predicate: dep,
                    object: Variable::new("C"),
                },
            ],
            confidence: 0.9,
        };

        let inferred = engine.infer(
            vec![
                Relation::new(EntityId(1), dep, EntityId(2), 0.9, Provenance::Memory),
                Relation::new(EntityId(2), dep, EntityId(3), 0.9, Provenance::Memory),
                Relation::new(EntityId(3), dep, EntityId(4), 0.9, Provenance::Memory),
            ],
            &[rule],
        );

        assert!(
            inferred
                .iter()
                .any(|relation| relation.subject == EntityId(1) && relation.object == EntityId(4))
        );
    }

    #[test]
    fn rule_result_is_invariant_to_pattern_order() {
        let dep = PredicateId(1);
        let relations = vec![
            Relation::new(EntityId(1), dep, EntityId(2), 0.8, Provenance::Memory),
            Relation::new(EntityId(2), dep, EntityId(3), 0.7, Provenance::Memory),
        ];
        let index = RelationIndex::build(&relations);

        let rule_a = Rule {
            head: RelationPattern {
                subject: Variable::new("A"),
                predicate: dep,
                object: Variable::new("C"),
            },
            body: vec![
                RelationPattern {
                    subject: Variable::new("A"),
                    predicate: dep,
                    object: Variable::new("B"),
                },
                RelationPattern {
                    subject: Variable::new("B"),
                    predicate: dep,
                    object: Variable::new("C"),
                },
            ],
            confidence: 0.9,
        };
        let rule_b = Rule {
            head: RelationPattern {
                subject: Variable::new("A"),
                predicate: dep,
                object: Variable::new("C"),
            },
            body: vec![
                RelationPattern {
                    subject: Variable::new("B"),
                    predicate: dep,
                    object: Variable::new("C"),
                },
                RelationPattern {
                    subject: Variable::new("A"),
                    predicate: dep,
                    object: Variable::new("B"),
                },
            ],
            confidence: 0.9,
        };

        assert_eq!(rule_a.apply_indexed(&index), rule_b.apply_indexed(&index));
    }

    #[test]
    fn indexed_and_scan_rule_application_match() {
        let dep = PredicateId(1);
        let relations = vec![
            Relation::new(EntityId(1), dep, EntityId(2), 0.8, Provenance::Memory),
            Relation::new(EntityId(2), dep, EntityId(3), 0.7, Provenance::Memory),
            Relation::new(EntityId(3), dep, EntityId(4), 0.6, Provenance::Memory),
        ];
        let index = RelationIndex::build(&relations);
        let rule = Rule {
            head: RelationPattern {
                subject: Variable::new("A"),
                predicate: dep,
                object: Variable::new("C"),
            },
            body: vec![
                RelationPattern {
                    subject: Variable::new("A"),
                    predicate: dep,
                    object: Variable::new("B"),
                },
                RelationPattern {
                    subject: Variable::new("B"),
                    predicate: dep,
                    object: Variable::new("C"),
                },
            ],
            confidence: 0.9,
        };

        assert_eq!(rule.apply_scan(&relations), rule.apply_indexed(&index));
    }

    #[test]
    fn memory_recall_improves_or_matches_inference() {
        let dep = PredicateId(1);
        let engine = TensorLogicEngine {
            max_steps: 0,
            threshold: 0.1,
        };
        let rule = Rule {
            head: RelationPattern {
                subject: Variable::new("A"),
                predicate: dep,
                object: Variable::new("C"),
            },
            body: vec![
                RelationPattern {
                    subject: Variable::new("A"),
                    predicate: dep,
                    object: Variable::new("B"),
                },
                RelationPattern {
                    subject: Variable::new("B"),
                    predicate: dep,
                    object: Variable::new("C"),
                },
            ],
            confidence: 0.9,
        };
        let controller = MemoryController {
            config: MemoryConfig {
                min_confidence: 0.1,
                decay_lambda: 0.0,
                max_memory_size: 8,
                recall_top_k: 1,
            },
        };
        let mut memory = MemorySpace::new();
        memory.store(
            Experience {
                input_relations: vec![
                    Relation::new(EntityId(1), dep, EntityId(2), 0.9, Provenance::Memory),
                    Relation::new(EntityId(2), dep, EntityId(3), 0.9, Provenance::Memory),
                ],
                inferred_relations: vec![Relation::new(
                    EntityId(1),
                    dep,
                    EntityId(3),
                    0.729,
                    Provenance::Inferred,
                )],
                rules_applied: vec![RuleId(0)],
                confidence: 0.729,
                timestamp: 1,
            },
            &controller,
            1,
        );

        let without_memory = engine.infer(
            vec![
                Relation::new(EntityId(1), dep, EntityId(2), 0.9, Provenance::Memory),
                Relation::new(EntityId(3), dep, EntityId(4), 0.9, Provenance::Memory),
            ],
            &[rule.clone()],
        );
        let with_memory = engine.infer_with_memory(
            vec![
                Relation::new(EntityId(1), dep, EntityId(2), 0.9, Provenance::Memory),
                Relation::new(EntityId(3), dep, EntityId(4), 0.9, Provenance::Memory),
            ],
            &[rule],
            &mut memory,
            &controller,
            2,
        );

        assert!(with_memory.len() >= without_memory.len());
        assert!(memory.experiences.len() >= 2);
    }

    #[test]
    fn memory_recall_deduplicates_relations() {
        let dep = PredicateId(1);
        let engine = TensorLogicEngine {
            max_steps: 0,
            threshold: 0.1,
        };
        let controller = MemoryController {
            config: MemoryConfig {
                min_confidence: 0.1,
                decay_lambda: 0.0,
                max_memory_size: 8,
                recall_top_k: 1,
            },
        };
        let mut memory = MemorySpace::new();
        memory.store(
            Experience {
                input_relations: vec![Relation::new(
                    EntityId(1),
                    dep,
                    EntityId(2),
                    0.9,
                    Provenance::Memory,
                )],
                inferred_relations: vec![Relation::new(
                    EntityId(1),
                    dep,
                    EntityId(2),
                    0.7,
                    Provenance::Inferred,
                )],
                rules_applied: vec![],
                confidence: 0.7,
                timestamp: 1,
            },
            &controller,
            1,
        );

        let output = engine.infer_with_memory(
            vec![Relation::new(
                EntityId(1),
                dep,
                EntityId(2),
                0.9,
                Provenance::Memory,
            )],
            &[],
            &mut memory,
            &controller,
            2,
        );

        assert_eq!(output.len(), 1);
        assert!((output[0].weight - 0.9).abs() < 1e-6);
    }

    #[test]
    fn memory_is_append_only_and_deterministic() {
        let dep = PredicateId(1);
        let engine = TensorLogicEngine {
            max_steps: 2,
            threshold: 0.1,
        };
        let rule = Rule {
            head: RelationPattern {
                subject: Variable::new("A"),
                predicate: dep,
                object: Variable::new("C"),
            },
            body: vec![
                RelationPattern {
                    subject: Variable::new("A"),
                    predicate: dep,
                    object: Variable::new("B"),
                },
                RelationPattern {
                    subject: Variable::new("B"),
                    predicate: dep,
                    object: Variable::new("C"),
                },
            ],
            confidence: 0.9,
        };
        let controller = MemoryController {
            config: MemoryConfig {
                min_confidence: 0.1,
                decay_lambda: 0.0,
                max_memory_size: 8,
                recall_top_k: 1,
            },
        };
        let input = vec![
            Relation::new(EntityId(1), dep, EntityId(2), 0.8, Provenance::Memory),
            Relation::new(EntityId(2), dep, EntityId(3), 0.7, Provenance::Memory),
        ];

        let mut memory_a = MemorySpace::new();
        let mut memory_b = MemorySpace::new();
        let output_a = engine.infer_with_memory(
            input.clone(),
            &[rule.clone()],
            &mut memory_a,
            &controller,
            10,
        );
        let output_b = engine.infer_with_memory(input, &[rule], &mut memory_b, &controller, 10);

        assert_eq!(output_a, output_b);
        assert_eq!(memory_a.experiences.len(), 1);
        assert_eq!(memory_b.experiences.len(), 1);
        assert_eq!(memory_a, memory_b);
    }

    #[test]
    fn memory_confidence_filter_rejects_low_confidence_experience() {
        let controller = MemoryController {
            config: MemoryConfig {
                min_confidence: 0.5,
                decay_lambda: 0.0,
                max_memory_size: 8,
                recall_top_k: 1,
            },
        };
        let mut memory = MemorySpace::new();
        memory.store(
            Experience {
                input_relations: Vec::new(),
                inferred_relations: Vec::new(),
                rules_applied: Vec::new(),
                confidence: 0.4,
                timestamp: 1,
            },
            &controller,
            1,
        );

        assert!(memory.experiences.is_empty());
    }

    #[test]
    fn memory_decay_reduces_confidence_over_time() {
        let controller = MemoryController {
            config: MemoryConfig {
                min_confidence: 0.0,
                decay_lambda: 0.5,
                max_memory_size: 8,
                recall_top_k: 1,
            },
        };
        let dep = PredicateId(1);
        let mut memory = MemorySpace::new();
        memory.store(
            Experience {
                input_relations: vec![Relation::new(
                    EntityId(1),
                    dep,
                    EntityId(2),
                    0.9,
                    Provenance::Memory,
                )],
                inferred_relations: vec![Relation::new(
                    EntityId(1),
                    dep,
                    EntityId(3),
                    0.8,
                    Provenance::Inferred,
                )],
                rules_applied: vec![RuleId(0)],
                confidence: 1.0,
                timestamp: 1,
            },
            &controller,
            1,
        );

        let recalled = memory.recall(
            &MemoryQuery {
                relations: vec![Relation::new(
                    EntityId(1),
                    dep,
                    EntityId(2),
                    0.9,
                    Provenance::Memory,
                )],
            },
            &controller,
            3,
        );

        assert_eq!(recalled.len(), 1);
        assert!(recalled[0].experience.confidence < 1.0);
    }

    #[test]
    fn memory_pruning_enforces_max_size() {
        let controller = MemoryController {
            config: MemoryConfig {
                min_confidence: 0.0,
                decay_lambda: 0.0,
                max_memory_size: 2,
                recall_top_k: 2,
            },
        };
        let mut memory = MemorySpace::new();

        for (confidence, timestamp) in [(0.2, 1_u64), (0.9, 2_u64), (0.5, 3_u64)] {
            memory.store(
                Experience {
                    input_relations: Vec::new(),
                    inferred_relations: Vec::new(),
                    rules_applied: Vec::new(),
                    confidence,
                    timestamp,
                },
                &controller,
                timestamp,
            );
        }

        assert_eq!(memory.experiences.len(), 2);
        assert!(
            memory
                .experiences
                .iter()
                .all(|experience| experience.confidence >= 0.5)
        );
    }

    #[test]
    fn memory_recall_excludes_low_confidence_after_stabilization() {
        let dep = PredicateId(1);
        let controller = MemoryController {
            config: MemoryConfig {
                min_confidence: 0.5,
                decay_lambda: 0.5,
                max_memory_size: 8,
                recall_top_k: 2,
            },
        };
        let mut memory = MemorySpace::new();
        memory.store(
            Experience {
                input_relations: vec![Relation::new(
                    EntityId(1),
                    dep,
                    EntityId(2),
                    0.9,
                    Provenance::Memory,
                )],
                inferred_relations: vec![Relation::new(
                    EntityId(1),
                    dep,
                    EntityId(3),
                    0.9,
                    Provenance::Inferred,
                )],
                rules_applied: vec![RuleId(0)],
                confidence: 0.9,
                timestamp: 1,
            },
            &controller,
            1,
        );

        let recalled = memory.recall(
            &MemoryQuery {
                relations: vec![Relation::new(
                    EntityId(1),
                    dep,
                    EntityId(2),
                    0.9,
                    Provenance::Memory,
                )],
            },
            &controller,
            10,
        );

        assert!(recalled.is_empty());
    }
}
