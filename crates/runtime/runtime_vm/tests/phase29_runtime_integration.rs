use runtime_core::{Phase9RuntimeContext, RuntimeEvent};
use runtime_vm::{ExecutionMode, HybridVm, Phase9RuntimeAdapter};

fn execute_phase29(input: &str) -> Phase9RuntimeContext {
    let mut vm = HybridVm::new(ExecutionMode::Reasoning);
    vm.set_input_text(input.to_string());
    vm.execute();
    Phase9RuntimeAdapter::from_legacy(vm.context())
}

fn event_index(events: &[RuntimeEvent], target: RuntimeEvent) -> usize {
    events
        .iter()
        .position(|event| *event == target)
        .expect("event must exist")
}

#[test]
fn phase29_pipeline_propagates_simulation_into_evaluation() {
    let phase = execute_phase29("phase29 runtime integration");
    let world_state = phase.world_state.as_ref().expect("world state");
    let simulation = world_state.simulation.as_ref().expect("simulation");
    let ai_context = phase.ai_context.as_ref().expect("ai context");
    let evaluation = ai_context
        .evaluation_state
        .latest
        .expect("evaluation result");
    let summary = phase.search_summary.as_ref().expect("search summary");

    assert_eq!(world_state.evaluation.simulation_quality, simulation.total());
    assert!(evaluation.total_score > 0.0);
    assert_eq!(summary.best_simulation_score, simulation.total());
    assert!(summary.best_score >= simulation.total() * 0.5);
}

#[test]
fn phase29_runtime_events_preserve_simulation_to_evaluation_order() {
    let phase = execute_phase29("phase29 telemetry ordering");
    let events = phase.event_bus.events().cloned().collect::<Vec<_>>();

    let pattern_match_started = event_index(&events, RuntimeEvent::PatternMatchStarted);
    let policy_eval_started = event_index(&events, RuntimeEvent::PolicyEvaluationStarted);
    let simulation_started = event_index(&events, RuntimeEvent::SimulationStarted);
    let simulation_completed = event_index(&events, RuntimeEvent::SimulationCompleted);
    let causal_analysis_started = event_index(&events, RuntimeEvent::CausalAnalysisStarted);
    let evaluation_started = event_index(&events, RuntimeEvent::EvaluationStarted);
    let policy_eval_completed = event_index(&events, RuntimeEvent::PolicyEvaluationCompleted);
    let evaluation_completed = event_index(&events, RuntimeEvent::EvaluationCompleted);

    assert!(pattern_match_started < policy_eval_started);
    assert!(policy_eval_started < simulation_started);
    assert!(simulation_started < simulation_completed);
    assert!(simulation_completed < causal_analysis_started);
    assert!(causal_analysis_started < evaluation_started);
    assert!(evaluation_started < policy_eval_completed);
    assert!(policy_eval_completed < evaluation_completed);
}

#[test]
fn phase29_learning_feedback_records_experience_after_simulation() {
    let phase = execute_phase29("phase29 learning feedback");
    let events = phase.event_bus.events().cloned().collect::<Vec<_>>();
    let ai_context = phase.ai_context.as_ref().expect("ai context");

    assert!(events.contains(&RuntimeEvent::ExperienceStored));
    assert!(events.contains(&RuntimeEvent::ExperienceGraphUpdated));
    assert!(events.contains(&RuntimeEvent::PolicyUpdated));
    assert_eq!(ai_context.experience_state.graph.edges.len(), 1);
    assert_eq!(ai_context.experience_state.graph.knowledges.len(), 1);
    assert_eq!(ai_context.experience_state.graph.lifecycle_states.len(), 1);
    assert_eq!(ai_context.experience_state.graph.lifecycle_metrics.len(), 1);
}
