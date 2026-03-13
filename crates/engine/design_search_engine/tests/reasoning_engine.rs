use concept_engine::ConceptId;
use design_search_engine::{ReasoningEngine, runtime_hypotheses_from_reasoning};
use memory_space_complex::ComplexField;
use memory_space_core::Complex64;

fn vector() -> ComplexField {
    ComplexField::new(vec![
        Complex64::new(1.0, 0.0),
        Complex64::new(0.5, 0.0),
        Complex64::new(0.25, 0.0),
        Complex64::new(0.125, 0.0),
    ])
}

#[test]
fn phase25_reasoning_engine_extracts_intent_and_generates_knowledge() {
    let result =
        ReasoningEngine::default().reason("Build Rust Web API with database and auth", vector());

    assert!(
        result
            .intent_graph
            .intents
            .iter()
            .any(|intent| intent == "Rust")
    );
    assert!(
        result
            .intent_graph
            .intents
            .iter()
            .any(|intent| intent == "REST API")
    );
    assert!(
        result
            .intent_graph
            .intents
            .iter()
            .any(|intent| intent == "Database")
    );
    assert!(!result.inferred_knowledge.is_empty());
}

#[test]
fn phase25_reasoning_engine_generates_bounded_valid_hypotheses() {
    let engine = ReasoningEngine::default();
    let result = engine.reason(
        "Build Rust Web API with database auth cache queue",
        vector(),
    );

    assert!(!result.architecture_hypotheses.is_empty());
    assert!(result.architecture_hypotheses.len() <= engine.config.max_hypotheses);
    assert!(
        result
            .architecture_hypotheses
            .iter()
            .all(|hypothesis| hypothesis.valid)
    );
    assert!(
        result
            .architecture_hypotheses
            .iter()
            .all(|hypothesis| !hypothesis.seed_state.design_units.is_empty())
    );
}

#[test]
fn phase25_reasoning_output_converts_to_search_seed_pairs() {
    let result =
        ReasoningEngine::default().reason("Build Rust Web API with database cache", vector());
    let runtime_pairs = runtime_hypotheses_from_reasoning(
        &result,
        &[
            ConceptId::from_name("DATABASE"),
            ConceptId::from_name("CACHE"),
            ConceptId::from_name("WEBSERVER"),
        ],
    );

    assert!(!runtime_pairs.is_empty());
    assert!(result.best_seed_state().is_some());
}
