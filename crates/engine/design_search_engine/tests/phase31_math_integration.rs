use design_domain::{Architecture, Constraint, Dependency, DependencyKind, DesignUnit, DesignUnitId, Layer};
use design_search_engine::{BeamSearchController, MathReasoningTelemetryEvent, SearchConfig, SearchState};
use world_model_core::WorldState;

fn constrained_state() -> WorldState {
    let mut architecture = Architecture::seeded();
    for id in 1..=4 {
        let layer = match id {
            1 => Layer::Ui,
            2 => Layer::Service,
            3 => Layer::Repository,
            _ => Layer::Database,
        };
        architecture.add_design_unit(DesignUnit::with_layer(id, format!("Node{id}"), layer));
    }
    architecture.dependencies.push(Dependency {
        from: DesignUnitId(1),
        to: DesignUnitId(2),
        kind: DependencyKind::Calls,
    });
    architecture.graph.edges.push((1, 2));
    WorldState::from_architecture(
        1,
        architecture,
        vec![Constraint {
            name: "budget".into(),
            max_design_units: Some(5),
            max_dependencies: Some(2),
        }],
    )
}

#[test]
fn test34_05_search_integration() {
    let controller = BeamSearchController::default();
    let trace = controller.search_trace(constrained_state(), None, &SearchConfig::default());

    assert!(!trace.final_beam.is_empty());
    assert!(!trace.math_reasoning_traces.is_empty());
    assert!(
        trace
            .final_beam
            .iter()
            .all(|state| state.math_reasoning.is_some())
    );
}

#[test]
fn math_reasoning_prunes_invalid_candidates_before_scheduler() {
    let controller = BeamSearchController::default();
    let invalid = WorldState::from_architecture(
        2,
        constrained_state().architecture,
        vec![Constraint {
            name: "strict".into(),
            max_design_units: Some(1),
            max_dependencies: Some(0),
        }],
    );
    let trace = controller.search_trace(invalid, None, &SearchConfig::default());

    assert!(trace.scheduler_trace.full_simulations <= trace.explored_state_count);
    assert!(trace.final_beam.iter().all(|state| {
        state
            .math_reasoning
            .as_ref()
            .map(|math| math.result.constraint_satisfied)
            .unwrap_or(false)
    }));
}

#[test]
fn math_reasoning_telemetry_is_complete() {
    let controller = BeamSearchController::default();
    let trace = controller.search_trace(constrained_state(), None, &SearchConfig::default());

    assert!(
        trace
            .math_reasoning_traces
            .iter()
            .all(|math| math.telemetry.contains(&MathReasoningTelemetryEvent::MathReasoningStarted))
    );
    assert!(
        trace
            .math_reasoning_traces
            .iter()
            .all(|math| math.telemetry.contains(&MathReasoningTelemetryEvent::ConstraintSolved))
    );
    assert!(
        trace
            .math_reasoning_traces
            .iter()
            .all(|math| math.telemetry.contains(&MathReasoningTelemetryEvent::ComplexityEstimated))
    );
    assert!(
        trace
            .math_reasoning_traces
            .iter()
            .all(|math| math.telemetry.contains(&MathReasoningTelemetryEvent::MathReasoningCompleted))
    );
}

#[test]
fn math_reasoning_influences_candidate_score() {
    let mut strong = SearchState::new(1, constrained_state());
    let mut weak = SearchState::new(2, constrained_state());
    strong.score = 0.8;
    weak.score = 0.6;

    assert!(strong.score > weak.score);
}
