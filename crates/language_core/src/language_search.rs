use design_domain::Constraint;

use crate::{
    ConceptMemory, LanguageAction, LanguageEvaluator, LanguageState, SemanticRelation,
    concept_memory::ConceptId, semantic_graph::RelationType,
};

pub fn language_search(initial_state: LanguageState) -> LanguageState {
    let evaluator = LanguageEvaluator;
    let memory = ConceptMemory::seeded();
    let mut beam = vec![initial_state];

    for _ in 0..4 {
        let mut candidates = Vec::new();
        for state in &beam {
            for action in [
                LanguageAction::InferIntent,
                LanguageAction::ExpandConcept,
                LanguageAction::AddRelation,
                LanguageAction::ResolveAmbiguity,
                LanguageAction::GenerateSentence,
            ] {
                candidates.push(apply_action(state, action, &memory));
            }
        }
        candidates.sort_by(|lhs, rhs| {
            evaluator
                .evaluate(rhs)
                .total()
                .total_cmp(&evaluator.evaluate(lhs).total())
                .then_with(|| lhs.source_text.cmp(&rhs.source_text))
        });
        candidates.dedup_by(|lhs, rhs| lhs == rhs);
        candidates.truncate(4);
        beam = candidates;
    }

    let mut best = beam
        .into_iter()
        .max_by(|lhs, rhs| {
            evaluator
                .evaluate(lhs)
                .total()
                .total_cmp(&evaluator.evaluate(rhs).total())
        })
        .unwrap_or_default();
    if best.generated_sentence.is_none() {
        best = apply_action(&best, LanguageAction::GenerateSentence, &memory);
    }
    best
}

pub fn semantic_graph_to_constraints(state: &LanguageState) -> Vec<Constraint> {
    let mut constraints = Vec::new();
    let labels = state
        .semantic_graph
        .concepts
        .values()
        .map(|concept| concept.label.as_str())
        .collect::<Vec<_>>();
    if labels.contains(&"api") {
        constraints.push(Constraint {
            name: "api_layers".to_string(),
            max_design_units: Some(16),
            max_dependencies: Some(24),
        });
    }
    if labels.contains(&"scalable") {
        constraints.push(Constraint {
            name: "scalable".to_string(),
            max_design_units: Some(24),
            max_dependencies: Some(32),
        });
    }
    if labels.contains(&"microservice") {
        constraints.push(Constraint {
            name: "microservice".to_string(),
            max_design_units: Some(32),
            max_dependencies: Some(48),
        });
    }
    constraints
}

fn apply_action(
    state: &LanguageState,
    action: LanguageAction,
    memory: &ConceptMemory,
) -> LanguageState {
    let mut next = state.clone();
    match action {
        LanguageAction::InferIntent => {
            if next.intent.is_none() && next.source_text.to_ascii_lowercase().contains("build") {
                next.intent = Some(semantic_domain::Intent {
                    name: "Build API".to_string(),
                    concepts: next
                        .semantic_graph
                        .concepts
                        .values()
                        .map(|concept| semantic_domain::Concept {
                            name: concept.label.clone(),
                            weight: 1.0,
                        })
                        .collect(),
                });
            }
        }
        LanguageAction::ExpandConcept => {
            let lower = next.source_text.to_ascii_lowercase();
            for concept in memory.resolve_text(&format!("{lower} service repository")) {
                next.semantic_field.activate(concept.concept_id, 0.9);
                next.semantic_graph.add_concept(concept);
            }
        }
        LanguageAction::AddRelation => {
            let ids = next
                .semantic_graph
                .concepts
                .keys()
                .copied()
                .collect::<Vec<_>>();
            if let (Some(api), Some(service)) = (
                find_concept(&next, "api"),
                find_concept(&next, "service").or_else(|| ids.get(1).copied()),
            ) {
                next.semantic_graph.add_relation(SemanticRelation {
                    source: api,
                    relation: RelationType::Requires,
                    target: service,
                });
            }
        }
        LanguageAction::ResolveAmbiguity => {
            for concept_id in next
                .semantic_graph
                .concepts
                .keys()
                .copied()
                .collect::<Vec<_>>()
            {
                let current = next.semantic_field.activation_of(concept_id);
                next.semantic_field
                    .activate(concept_id, (current + 0.1).clamp(0.0, 1.0));
            }
        }
        LanguageAction::GenerateSentence => {
            let mut labels = next
                .semantic_graph
                .concepts
                .values()
                .map(|concept| concept.label.clone())
                .collect::<Vec<_>>();
            labels.sort();
            next.generated_sentence = Some(format!(
                "{} {}",
                next.intent
                    .as_ref()
                    .map(|intent| intent.name.as_str())
                    .unwrap_or("Design"),
                labels.join(" ")
            ));
        }
    }
    next
}

fn find_concept(state: &LanguageState, label: &str) -> Option<ConceptId> {
    state
        .semantic_graph
        .concepts
        .values()
        .find(|concept| concept.label == label)
        .map(|concept| concept.concept_id)
}
