use language_core::{Concept, ConceptId, RelationType, SemanticGraph, SemanticRelation};

pub fn infer_semantic_relations(concept: ConceptId) -> Vec<SemanticRelation> {
    match concept.0 {
        2 => vec![
            relation(2, RelationType::Requires, 11),
            relation(2, RelationType::Uses, 12),
            relation(2, RelationType::Pattern, 13),
        ],
        6 => vec![
            relation(6, RelationType::Constrains, 11),
            relation(6, RelationType::Constrains, 14),
        ],
        5 => vec![
            relation(5, RelationType::Requires, 15),
            relation(5, RelationType::Uses, 16),
            relation(5, RelationType::Uses, 17),
        ],
        1 => vec![relation(1, RelationType::Pattern, 14)],
        _ => Vec::new(),
    }
}

pub fn inferred_concepts_for(concept: ConceptId) -> Vec<Concept> {
    infer_semantic_relations(concept)
        .into_iter()
        .map(|relation| concept_for(relation.target))
        .collect()
}

pub fn concept_for(concept_id: ConceptId) -> Concept {
    match concept_id.0 {
        11 => concept(11, "stateless", &["cacheless"]),
        12 => concept(12, "http", &["protocol"]),
        13 => concept(13, "client_server", &["request-response"]),
        14 => concept(14, "layered_architecture", &["layered"]),
        15 => concept(15, "service_discovery", &["registry"]),
        16 => concept(16, "api_gateway", &["gateway"]),
        17 => concept(17, "containerization", &["container"]),
        _ => concept(concept_id.0, "derived", &[]),
    }
}

pub fn apply_inference(graph: &mut SemanticGraph, concept_id: ConceptId) -> Vec<SemanticRelation> {
    let relations = infer_semantic_relations(concept_id);
    for concept in inferred_concepts_for(concept_id) {
        graph.add_concept(concept);
    }
    for relation in &relations {
        graph.add_relation(relation.clone());
    }
    relations
}

fn relation(source: u64, relation: RelationType, target: u64) -> SemanticRelation {
    SemanticRelation {
        source: ConceptId(source),
        relation,
        target: ConceptId(target),
    }
}

fn concept(id: u64, label: &str, attributes: &[&str]) -> Concept {
    Concept {
        concept_id: ConceptId(id),
        label: label.to_string(),
        attributes: attributes.iter().map(|attr| attr.to_string()).collect(),
    }
}
