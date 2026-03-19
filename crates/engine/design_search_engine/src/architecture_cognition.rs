use architecture_evaluator_core::{ArchitectureEvaluator, DefaultArchitectureEvaluator};
use architecture_reasoner::ReverseArchitectureReasoner;
use architecture_state_v2::ArchitectureState;
use code_ir::CodeIr;
use knowledge_engine::{KnowledgeEngine, KnowledgeQuery, knowledge_graph_to_constraints};

use crate::{SearchConfig, SearchState};

#[derive(Clone, Debug, PartialEq)]
pub struct ArchitectureCognitionSnapshot {
    pub state_id: u64,
    pub architecture_state: ArchitectureState,
    pub score: f64,
}

#[derive(Default)]
pub struct ArchitectureCognitionSearchIntegration {
    knowledge_engine: KnowledgeEngine,
    reasoner: ReverseArchitectureReasoner,
    evaluator: DefaultArchitectureEvaluator,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KnowledgeConstrainedSearchPlan {
    pub config: SearchConfig,
    pub constraint_count: usize,
    pub confidence: f64,
}

impl ArchitectureCognitionSearchIntegration {
    pub fn snapshot(
        &self,
        search_state: &SearchState,
        problem: impl Into<String>,
    ) -> ArchitectureCognitionSnapshot {
        let problem = problem.into();
        let knowledge = self.knowledge_engine.process_query(KnowledgeQuery {
            text: problem.clone(),
            semantic_hints: Vec::new(),
            semantic_vector: Vec::new(),
            keywords: Vec::new(),
            relation_types: Vec::new(),
            max_results: 64,
            confidence_threshold: 0.0,
        });
        let code_ir = CodeIr::from_architecture(&search_state.world_state.architecture);
        let architecture_graph = self.reasoner.infer_from_code_ir(&code_ir);
        let mut architecture_state = ArchitectureState::new(problem)
            .with_knowledge(knowledge)
            .with_constraints(search_state.world_state.constraints.clone());
        architecture_state.code_ir = code_ir;
        architecture_state.architecture_graph = architecture_graph;
        architecture_state.evaluation = Some(self.evaluator.evaluate(&architecture_state));

        ArchitectureCognitionSnapshot {
            state_id: search_state.state_id,
            score: architecture_state
                .evaluation
                .as_ref()
                .map(|evaluation| evaluation.overall)
                .unwrap_or_default(),
            architecture_state,
        }
    }

    pub fn knowledge_constrained_plan(
        &self,
        problem: impl Into<String>,
        base: SearchConfig,
    ) -> KnowledgeConstrainedSearchPlan {
        let knowledge = self.knowledge_engine.process_query(KnowledgeQuery {
            text: problem.into(),
            semantic_hints: Vec::new(),
            semantic_vector: Vec::new(),
            keywords: Vec::new(),
            relation_types: Vec::new(),
            max_results: 64,
            confidence_threshold: 0.0,
        });
        let constraints = knowledge_graph_to_constraints(&knowledge.knowledge_graph);
        let confidence = knowledge.validation.confidence;
        let has_constraints = !constraints.is_empty();
        let candidate_scale = if has_constraints {
            0.25
        } else if confidence >= 0.6 {
            0.375
        } else {
            0.6
        };
        let beam_scale = if has_constraints {
            0.5
        } else if confidence >= 0.6 {
            0.5
        } else {
            0.75
        };

        KnowledgeConstrainedSearchPlan {
            config: SearchConfig {
                max_depth: base.max_depth,
                max_candidates: ((base.max_candidates as f64 * candidate_scale).round() as usize)
                    .max(1),
                beam_width: ((base.beam_width as f64 * beam_scale).round() as usize).max(1),
                diversity_threshold: base.diversity_threshold,
                experience_bias: (base.experience_bias + confidence * 0.2).clamp(0.0, 1.0),
                policy_bias: (base.policy_bias + confidence * 0.2).clamp(0.0, 1.0),
            },
            constraint_count: constraints.len(),
            confidence,
        }
    }
}

#[cfg(test)]
mod tests {
    use design_domain::{
        Architecture, Constraint, Dependency, DependencyKind, DesignUnit, DesignUnitId,
    };
    use world_model_core::WorldState;

    use super::*;

    #[test]
    fn snapshot_contains_code_ir_and_geometry_evaluation() {
        let mut architecture = Architecture::seeded();
        architecture.add_design_unit(DesignUnit::new(2, "UserService"));
        architecture.add_design_unit(DesignUnit::new(3, "UserRepository"));
        architecture.dependencies.push(Dependency {
            from: DesignUnitId(2),
            to: DesignUnitId(3),
            kind: DependencyKind::Calls,
        });
        let world_state = WorldState::from_architecture(
            1,
            architecture,
            vec![Constraint {
                name: "SmallGraph".into(),
                max_design_units: Some(4),
                max_dependencies: Some(3),
            }],
        );
        let search_state = SearchState::new(1, world_state);

        let snapshot = ArchitectureCognitionSearchIntegration::default()
            .snapshot(&search_state, "design user api");

        assert_eq!(snapshot.architecture_state.code_ir.modules.len(), 2);
        assert!(snapshot.score > 0.0);
    }
}
