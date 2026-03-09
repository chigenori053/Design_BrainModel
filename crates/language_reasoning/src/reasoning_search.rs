use design_domain::Constraint;
use language_core::{ConceptId, SemanticGraph};

use crate::{
    ReasoningAction, ReasoningEvaluator, ReasoningState, concept_reasoning::expand_concepts,
    semantic_inference::apply_inference,
};

pub fn search(initial_graph: SemanticGraph) -> SemanticGraph {
    let evaluator = ReasoningEvaluator;
    let mut beam = vec![ReasoningState::new(initial_graph)];

    for _ in 0..3 {
        let mut candidates = Vec::new();
        for state in &beam {
            for action in [
                ReasoningAction::InferConstraint,
                ReasoningAction::InferArchitecturePattern,
                ReasoningAction::InferDependency,
                ReasoningAction::ExpandConcept,
                ReasoningAction::ResolveAmbiguity,
            ] {
                candidates.push(apply_action(state, action, &evaluator));
            }
        }
        candidates.sort_by(|lhs, rhs| {
            evaluator
                .evaluate(rhs)
                .total()
                .total_cmp(&evaluator.evaluate(lhs).total())
                .then_with(|| {
                    lhs.semantic_graph
                        .relations
                        .len()
                        .cmp(&rhs.semantic_graph.relations.len())
                })
        });
        candidates.dedup_by(|lhs, rhs| lhs == rhs);
        candidates.truncate(4);
        beam = candidates;
    }

    beam.into_iter()
        .max_by(|lhs, rhs| {
            evaluator
                .evaluate(lhs)
                .total()
                .total_cmp(&evaluator.evaluate(rhs).total())
        })
        .unwrap_or_default()
        .semantic_graph
}

pub fn reasoning_graph_to_constraints(graph: &SemanticGraph) -> Vec<Constraint> {
    let labels = graph
        .concepts
        .values()
        .map(|concept| concept.label.as_str())
        .collect::<Vec<_>>();
    let mut constraints = Vec::new();
    if labels.contains(&"stateless") {
        constraints.push(Constraint {
            name: "stateless".to_string(),
            max_design_units: Some(20),
            max_dependencies: Some(20),
        });
    }
    if labels.contains(&"layered_architecture") {
        constraints.push(Constraint {
            name: "layered_architecture".to_string(),
            max_design_units: Some(24),
            max_dependencies: Some(28),
        });
    }
    if labels.contains(&"api_gateway") {
        constraints.push(Constraint {
            name: "api_gateway".to_string(),
            max_design_units: Some(28),
            max_dependencies: Some(36),
        });
    }
    constraints
}

fn apply_action(
    state: &ReasoningState,
    action: ReasoningAction,
    evaluator: &ReasoningEvaluator,
) -> ReasoningState {
    let mut next = state.clone();
    match action {
        ReasoningAction::InferConstraint => {
            for concept_id in
                concept_ids_by_label(&next.semantic_graph, &["rest", "scalable", "api"])
            {
                next.inferred_relations
                    .extend(apply_inference(&mut next.semantic_graph, concept_id));
            }
        }
        ReasoningAction::InferArchitecturePattern => {
            for concept_id in concept_ids_by_label(&next.semantic_graph, &["api", "scalable"]) {
                next.inferred_relations
                    .extend(apply_inference(&mut next.semantic_graph, concept_id));
            }
        }
        ReasoningAction::InferDependency => {
            for concept_id in concept_ids_by_label(&next.semantic_graph, &["microservice"]) {
                next.inferred_relations
                    .extend(apply_inference(&mut next.semantic_graph, concept_id));
            }
        }
        ReasoningAction::ExpandConcept => {
            next.semantic_graph = expand_concepts(&next.semantic_graph);
            next.inferred_relations = next.semantic_graph.relations.clone();
        }
        ReasoningAction::ResolveAmbiguity => {
            next.semantic_graph
                .relations
                .sort_by_key(|relation| (relation.source, relation.target, relation.relation));
            next.semantic_graph.relations.dedup();
        }
    }
    next.reasoning_score = evaluator.evaluate(&next).total();
    next
}

fn concept_ids_by_label(graph: &SemanticGraph, labels: &[&str]) -> Vec<ConceptId> {
    graph
        .concepts
        .values()
        .filter(|concept| labels.contains(&concept.label.as_str()))
        .map(|concept| concept.concept_id)
        .collect()
}
