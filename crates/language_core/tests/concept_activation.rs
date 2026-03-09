use language_core::ConceptMemory;

#[test]
fn concept_memory_grounds_known_terms() {
    let memory = ConceptMemory::seeded();
    let concepts = memory.resolve_text("scalable microservice api with authentication");
    let labels = concepts
        .iter()
        .map(|concept| concept.label.as_str())
        .collect::<Vec<_>>();

    assert!(labels.contains(&"scalable"));
    assert!(labels.contains(&"microservice"));
    assert!(labels.contains(&"api"));
    assert!(labels.contains(&"authentication"));
}
