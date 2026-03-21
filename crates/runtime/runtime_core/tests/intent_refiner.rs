use std::sync::Arc;

use design_search_engine::stable_v03::DeterministicBeamSearchEngine;
use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord};
use runtime_core::intent_refiner::{
    ChatContext, CoreSlot, DefaultIntentRefiner, IntentExecution, IntentRefiner, SlotSource,
};
use runtime_core::{CoreRuntime, RuntimeExecutionResult};

fn empty_context() -> ChatContext {
    ChatContext::default()
}

#[test]
fn ready_case_returns_structured_intent() {
    let memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    let refiner = DefaultIntentRefiner::new(memory);

    let execution = refiner
        .refine("build rust api", &empty_context())
        .expect("intent refinement should succeed");

    match execution {
        IntentExecution::Ready(intent) => {
            assert_eq!(intent.goal, "build rust api");
            assert_eq!(
                intent
                    .slots
                    .core
                    .get(&CoreSlot::InterfaceType)
                    .unwrap()
                    .value,
                "api"
            );
            assert_eq!(
                intent.slots.core.get(&CoreSlot::Language).unwrap().value,
                "rust"
            );
            assert_eq!(
                intent.slots.core.get(&CoreSlot::Framework).unwrap().value,
                "axum"
            );
            assert_eq!(
                intent.slots.core.get(&CoreSlot::Framework).unwrap().source,
                SlotSource::Default
            );
        }
        IntentExecution::NeedClarification(_) => panic!("intent should be ready"),
    }
}

#[test]
fn clarification_case_returns_language_and_framework() {
    let memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    let refiner = DefaultIntentRefiner::new(memory);

    let execution = refiner
        .refine("build api", &empty_context())
        .expect("intent refinement should succeed");

    match execution {
        IntentExecution::Ready(_) => panic!("intent should require clarification"),
        IntentExecution::NeedClarification(clarification) => {
            assert_eq!(
                clarification.missing,
                vec![CoreSlot::Language, CoreSlot::Framework]
            );
            assert_eq!(
                clarification.message,
                "Which language and framework do you want? (rust+axum/typescript+express/go+gin)"
            );
        }
    }
}

#[test]
fn synonym_postgresql_maps_deterministically_without_affecting_required_slots() {
    let memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    let refiner = DefaultIntentRefiner::new(memory);

    let (execution, trace) = refiner
        .refine_with_trace("build rust api with postgresql", &empty_context())
        .expect("intent refinement should succeed");

    match execution {
        IntentExecution::Ready(intent) => {
            assert!(trace.tokens.iter().any(|token| token == "postgres"));
            assert_eq!(
                intent.slots.core.get(&CoreSlot::Language).unwrap().value,
                "rust"
            );
        }
        IntentExecution::NeedClarification(_) => panic!("intent should be ready"),
    }
}

#[test]
fn memory_can_fill_language_and_framework() {
    let memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    memory.store(MemoryRecord {
        id: "rust-api".to_string(),
        text: "rust axum api".to_string(),
        tags: vec!["rust".to_string(), "axum".to_string(), "api".to_string()],
        embedding: None,
        architecture: None,
        relations: vec!["seed".to_string()],
    });
    let refiner = DefaultIntentRefiner::new(memory);

    let execution = refiner
        .refine("build api", &empty_context())
        .expect("intent refinement should succeed");

    match execution {
        IntentExecution::Ready(intent) => {
            assert_eq!(
                intent.slots.core.get(&CoreSlot::Language).unwrap().value,
                "rust"
            );
            assert_eq!(
                intent.slots.core.get(&CoreSlot::Language).unwrap().source,
                SlotSource::Memory
            );
            assert_eq!(
                intent.slots.core.get(&CoreSlot::Framework).unwrap().value,
                "axum"
            );
            assert_eq!(
                intent.slots.core.get(&CoreSlot::Framework).unwrap().source,
                SlotSource::Memory
            );
        }
        IntentExecution::NeedClarification(_) => panic!("memory should complete the slots"),
    }
}

#[test]
fn refinement_is_deterministic_over_repeated_runs() {
    let memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    let refiner = DefaultIntentRefiner::new(memory);
    let expected = refiner
        .refine("build api", &empty_context())
        .expect("intent refinement should succeed");

    for _ in 0..1000 {
        let actual = refiner
            .refine("build api", &empty_context())
            .expect("intent refinement should succeed");
        assert_eq!(actual, expected);
    }
}

#[test]
fn runtime_execute_from_text_returns_executed_with_trace() {
    let runtime = CoreRuntime::new_with_defaults(
        Arc::new(InMemoryEngine::default()),
        Arc::new(DeterministicBeamSearchEngine::default()),
    );

    let result = runtime
        .execute_from_text("build rust api", &empty_context())
        .expect("runtime should succeed");

    match result {
        RuntimeExecutionResult::Executed(runtime_result) => {
            assert!(runtime_result.intent_trace.is_some());
            assert!(runtime_result.explanation.is_some());
            assert!(runtime_result.reasoning_trace.is_some());
            assert!(runtime_result.trace.generated_hypotheses > 0);
            assert!(runtime_result.trace.search_depth <= 2);
            assert!(runtime_result.reasoning_trace.as_ref().unwrap().stats.total_nodes > 0);
            let trace = runtime_result.intent_trace.unwrap();
            assert_eq!(
                trace
                    .final_slots
                    .core
                    .get(&CoreSlot::Framework)
                    .unwrap()
                    .value,
                "axum"
            );
        }
        RuntimeExecutionResult::Clarification(_) => panic!("runtime should execute"),
    }
}

#[test]
fn explanation_marks_slot_sources_deterministically() {
    let runtime = CoreRuntime::new_with_defaults(
        Arc::new(InMemoryEngine::default()),
        Arc::new(DeterministicBeamSearchEngine::default()),
    );

    let result = runtime
        .execute_from_text("build rust api", &empty_context())
        .expect("runtime should succeed");

    match result {
        RuntimeExecutionResult::Executed(runtime_result) => {
            let explanation = runtime_result.explanation.expect("explanation");
            assert!(explanation.intent.iter().any(|slot| {
                slot.slot == "Language"
                    && slot.value == "rust"
                    && slot.source == SlotSource::Explicit
            }));
            assert!(explanation.intent.iter().any(|slot| {
                slot.slot == "Framework"
                    && slot.value == "axum"
                    && slot.source == SlotSource::Default
            }));
        }
        RuntimeExecutionResult::Clarification(_) => panic!("runtime should execute"),
    }
}

#[test]
fn explanation_includes_inference_decision_for_api_keyword() {
    let runtime = CoreRuntime::new_with_defaults(
        Arc::new(InMemoryEngine::default()),
        Arc::new(DeterministicBeamSearchEngine::default()),
    );

    let result = runtime
        .execute_from_text("build rust api", &empty_context())
        .expect("runtime should succeed");

    match result {
        RuntimeExecutionResult::Executed(runtime_result) => {
            let explanation = runtime_result.explanation.expect("explanation");
            assert!(
                explanation
                    .decisions
                    .iter()
                    .any(|decision| decision.message == "Interface inferred from keyword 'api'")
            );
        }
        RuntimeExecutionResult::Clarification(_) => panic!("runtime should execute"),
    }
}

#[test]
fn explanation_is_deterministic_for_same_trace() {
    let lhs = CoreRuntime::new_with_defaults(
        Arc::new(InMemoryEngine::default()),
        Arc::new(DeterministicBeamSearchEngine::default()),
    )
    .execute_from_text("build rust api", &empty_context())
    .expect("runtime should succeed");
    let rhs = CoreRuntime::new_with_defaults(
        Arc::new(InMemoryEngine::default()),
        Arc::new(DeterministicBeamSearchEngine::default()),
    )
    .execute_from_text("build rust api", &empty_context())
    .expect("runtime should succeed");

    match (lhs, rhs) {
        (
            RuntimeExecutionResult::Executed(lhs_result),
            RuntimeExecutionResult::Executed(rhs_result),
        ) => assert_eq!(lhs_result.explanation, rhs_result.explanation),
        _ => panic!("runtime should execute in both cases"),
    }
}

#[test]
fn runtime_execute_from_text_returns_clarification() {
    let runtime = CoreRuntime::new_with_defaults(
        Arc::new(InMemoryEngine::default()),
        Arc::new(DeterministicBeamSearchEngine::default()),
    );

    let result = runtime
        .execute_from_text("build api", &empty_context())
        .expect("runtime should return clarification without error");

    match result {
        RuntimeExecutionResult::Executed(_) => panic!("runtime should not execute"),
        RuntimeExecutionResult::Clarification(clarification) => {
            assert_eq!(
                clarification.missing,
                vec![CoreSlot::Language, CoreSlot::Framework]
            );
            assert_eq!(
                clarification.message,
                "Which language and framework do you want? (rust+axum/typescript+express/go+gin)"
            );
        }
    }
}
