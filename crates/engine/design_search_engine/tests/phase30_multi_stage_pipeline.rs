use design_search_engine::{BeamSearchController, SchedulerTelemetryEvent, SearchConfig};
use world_model_core::WorldState;

#[test]
fn test33_07_multi_stage_pipeline_integrates_scheduler() {
    let controller = BeamSearchController::default();
    let trace = controller.search_trace(
        WorldState::new(1, vec![4.0, 1.0]),
        None,
        &SearchConfig {
            max_depth: 2,
            max_candidates: 16,
            beam_width: 4,
            diversity_threshold: 0.85,
            experience_bias: 0.2,
            policy_bias: 0.15,
        },
    );

    assert!(!trace.final_beam.is_empty());
    assert!(trace.scheduler_trace.knowledge_evaluated > 0);
    assert!(trace.scheduler_trace.light_simulated > 0);
    assert!(trace.scheduler_trace.scheduled_candidates > 0);
    assert!(
        trace
            .scheduler_trace
            .telemetry_events
            .iter()
            .any(|event| matches!(event, SchedulerTelemetryEvent::SimulationScheduled(_)))
    );
    assert!(
        trace.scheduler_trace.full_simulations <= trace.explored_state_count,
        "full={} explored={}",
        trace.scheduler_trace.full_simulations,
        trace.explored_state_count
    );
    assert!(
        trace
            .final_beam
            .iter()
            .all(|state| state.world_state.simulation.is_some())
    );
}
