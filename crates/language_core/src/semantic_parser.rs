use semantic_domain::{Concept as SemanticConcept, Intent};

use crate::{
    ConceptMemory, LanguageState, SemanticField, SemanticGraph, SemanticRelation,
    semantic_graph::RelationType,
};

pub fn semantic_parser(input: &str) -> LanguageState {
    let memory = ConceptMemory::seeded();
    let concepts = memory.resolve_text(input);
    let mut graph = SemanticGraph::default();
    let mut field = SemanticField::default();

    for concept in concepts {
        field.activate(concept.concept_id, 0.8);
        graph.add_concept(concept);
    }

    connect_known_patterns(&mut graph);

    let intent = infer_intent(input, &graph);
    LanguageState {
        semantic_graph: graph,
        semantic_field: field,
        intent,
        generated_sentence: None,
        source_text: input.to_string(),
    }
}

fn connect_known_patterns(graph: &mut SemanticGraph) {
    let ids = graph.concepts.keys().copied().collect::<Vec<_>>();
    for source in &ids {
        for target in &ids {
            if source == target {
                continue;
            }
            let source_label = &graph.concepts[source].label;
            let target_label = &graph.concepts[target].label;
            let relation = match (source_label.as_str(), target_label.as_str()) {
                ("api", "database") => Some(RelationType::Uses),
                ("api", "authentication") => Some(RelationType::Requires),
                ("rest", "api") => Some(RelationType::Pattern),
                ("scalable", "api") | ("scalable", "microservice") => {
                    Some(RelationType::Constrains)
                }
                ("microservice", "service") => Some(RelationType::Clarifies),
                _ => None,
            };
            if let Some(relation) = relation {
                graph.add_relation(SemanticRelation {
                    source: *source,
                    relation,
                    target: *target,
                });
            }
        }
    }
}

fn infer_intent(input: &str, graph: &SemanticGraph) -> Option<Intent> {
    let lower = input.to_ascii_lowercase();
    let name = if lower.contains("build")
        || lower.contains("design")
        || lower.contains("construct")
        || lower.contains("construir")
        || input.contains("構築")
        || input.contains("設計")
    {
        Some("Build API")
    } else if lower.contains("generate") {
        Some("Generate Design")
    } else {
        None
    }?;
    Some(Intent {
        name: name.to_string(),
        concepts: graph
            .concepts
            .values()
            .map(|concept| SemanticConcept {
                name: concept.label.clone(),
                weight: 1.0,
            })
            .collect(),
    })
}
