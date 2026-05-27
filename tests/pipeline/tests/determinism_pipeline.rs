use std::sync::Arc;

use design_search_engine::stable_v03::{DesignSearchEngine, DeterministicBeamSearchEngine};
use memory_engine::{InMemoryEngine, MemoryEngine, MemoryRecord};
use runtime_core::CoreRuntime;
use world_model::stable_v03::IntentInput;

#[test]
fn search_pipeline_is_fully_deterministic_for_same_input() {
    let engine = DeterministicBeamSearchEngine::default();
    let input = engine.contract_input(
        &world_model::stable_v03::IntentState {
            raw: "api service db".to_string(),
            tokens: vec!["api".to_string(), "service".to_string(), "db".to_string()],
        },
        None,
    );

    let lhs = engine.search_with_trace(input.clone());
    let rhs = engine.search_with_trace(input);

    assert_eq!(lhs.trace, rhs.trace);
    assert_eq!(lhs.candidates, rhs.candidates);
    assert_eq!(lhs.validation, rhs.validation);
}

#[test]
fn runtime_results_remain_equal_across_repeated_runs() {
    let make_runtime = || {
        let memory = Arc::new(InMemoryEngine::default());
        memory.store(MemoryRecord {
            id: "seed".to_string(),
            text: "api service db".to_string(),
            tags: vec!["api".to_string(), "service".to_string(), "db".to_string()],
            embedding: None,
            architecture: None,
            relations: Vec::new(),
        });
        CoreRuntime::new_with_defaults(
            memory as Arc<dyn MemoryEngine>,
            Arc::new(DeterministicBeamSearchEngine::default()) as Arc<dyn DesignSearchEngine>,
        )
    };

    let cold = make_runtime()
        .executor
        .execute(IntentInput::new("api service db"))
        .expect("cold run succeeds");
    let warm = make_runtime()
        .executor
        .execute(IntentInput::new("api service db"))
        .expect("second run succeeds");

    assert_eq!(cold.architecture, warm.architecture);
    assert_eq!(cold.trace, warm.trace);
    assert_eq!(cold.reasoning_trace, warm.reasoning_trace);
}
