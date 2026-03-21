use std::sync::Arc;

use design_search_engine::stable_v03::{
    DesignSearchEngine, DeterministicBeamSearchEngine, ReasoningCore,
};
use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine};
use runtime_core::CoreRuntime;
use world_model::stable_v03::IntentInput;

#[test]
fn pipeline_e2e_contract_holds() {
    let engine = DeterministicBeamSearchEngine::default();
    let input = engine.contract_input(
        &world_model::stable_v03::IntentState {
            raw: "api service db".to_string(),
            tokens: vec!["api".to_string(), "service".to_string(), "db".to_string()],
        },
        None,
    );

    let result = engine.reason(input);

    assert!(!result.trace.steps.is_empty());
    assert!((0.0..=1.0).contains(&result.confidence));
}

#[test]
fn failure_paths_are_observable_in_pipeline_outputs() {
    let engine = DeterministicBeamSearchEngine {
        early_termination_score: 0.2,
        early_goal_distance: 1.0,
        max_depth: 5,
        ..DeterministicBeamSearchEngine::default()
    };
    let input = engine.contract_input(
        &world_model::stable_v03::IntentState {
            raw: "".to_string(),
            tokens: Vec::new(),
        },
        None,
    );
    let result = engine.reason(input);

    assert!(!result.trace.steps.is_empty());
    assert!(result.trace.stats.recall_hit_rate <= 1.0);
    assert!(result.trace.steps.len() < 1 + engine.max_depth);
}

#[test]
fn runtime_recall_miss_is_reflected_without_breaking_trace() {
    let runtime = CoreRuntime::new_with_defaults(
        Arc::new(InMemoryEngine::default()) as Arc<dyn MemoryEngine>,
        Arc::new(DeterministicBeamSearchEngine::default()) as Arc<dyn DesignSearchEngine>,
    );
    let result = runtime
        .executor
        .execute(IntentInput::new("api service db"))
        .expect("runtime succeeds");
    let trace = result.reasoning_trace.expect("reasoning trace");

    assert!(!trace.steps.is_empty());
    assert_eq!(trace.stats, contracts::TraceStats::from_steps(&trace.steps));
}
