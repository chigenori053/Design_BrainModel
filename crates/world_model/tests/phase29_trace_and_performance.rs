use std::thread;
use std::time::Instant;

use design_domain::{Architecture, Constraint, DesignUnit, Dependency, DependencyKind, DesignUnitId};
use world_model::{DefaultSimulationEngine, TracedSimulation};
use world_model_core::{SimulationTelemetryEventKind, WorldState};

fn large_architecture(node_count: u64) -> Architecture {
    let mut architecture = Architecture::seeded();
    for unit_id in 1..=node_count {
        architecture.add_design_unit(DesignUnit::new(unit_id, format!("Node{unit_id}")));
    }
    for unit_id in 1..node_count {
        architecture.dependencies.push(Dependency {
            from: DesignUnitId(unit_id),
            to: DesignUnitId(unit_id + 1),
            kind: DependencyKind::Calls,
        });
        architecture.graph.edges.push((unit_id, unit_id + 1));
    }
    architecture
}

fn traced_simulation(state_id: u64) -> TracedSimulation {
    let state = WorldState::from_architecture(
        state_id,
        large_architecture(8),
        vec![Constraint {
            name: "dependency-budget".into(),
            max_design_units: Some(16),
            max_dependencies: Some(10),
        }],
    );
    DefaultSimulationEngine.simulate_with_trace(&state, None)
}

#[test]
fn phase29_traced_simulation_emits_started_step_completed_events() {
    let traced = traced_simulation(31);
    let step_count = traced
        .telemetry_events
        .iter()
        .filter(|event| event.kind == SimulationTelemetryEventKind::Step)
        .count();

    assert_eq!(
        traced.telemetry_events.first().expect("started").kind,
        SimulationTelemetryEventKind::Started
    );
    assert_eq!(
        traced.telemetry_events.last().expect("completed").kind,
        SimulationTelemetryEventKind::Completed
    );
    assert!(step_count > 0);
    assert_eq!(step_count, traced.traces.simulation_trace.step_count);
}

#[test]
fn phase29_trace_bundle_is_complete_and_correlated() {
    let traced = traced_simulation(32);
    let simulation_id = traced.traces.simulation_id();

    assert!(traced.traces.trace_complete);
    assert_eq!(traced.traces.simulation_trace.simulation_id, simulation_id);
    assert_eq!(
        traced.traces.state_transition_trace.simulation_id,
        simulation_id
    );
    assert_eq!(
        traced.traces.constraint_validation_trace.simulation_id,
        simulation_id
    );
    assert_eq!(
        traced.traces.behavior_prediction_trace.simulation_id,
        simulation_id
    );
}

#[test]
fn phase29_telemetry_event_count_covers_all_steps() {
    let traced = traced_simulation(33);
    let expected_steps = traced.traces.simulation_trace.step_count;
    let event_count = traced.telemetry_events.len();

    assert!(event_count >= expected_steps + 2);
}

#[test]
fn phase29_telemetry_sequence_is_deterministic_for_same_world_state() {
    let state = WorldState::from_architecture(34, large_architecture(8), Vec::new());

    let left = DefaultSimulationEngine.simulate_with_trace(&state, None);
    let right = DefaultSimulationEngine.simulate_with_trace(&state, None);

    assert_eq!(left.telemetry_events, right.telemetry_events);
    assert_eq!(left.traces, right.traces);
    assert_eq!(left.result, right.result);
}

#[test]
fn phase29_average_latency_for_hundred_simulations_stays_within_threshold() {
    let state = WorldState::from_architecture(35, large_architecture(32), Vec::new());
    let started = Instant::now();
    for _ in 0..100 {
        let _ = DefaultSimulationEngine.simulate_with_trace(&state, None);
    }
    let average = started.elapsed() / 100;

    assert!(average.as_millis() < 50, "avg latency was {:?}", average);
}

#[test]
fn phase29_parallel_simulations_complete_without_panic() {
    let state = WorldState::from_architecture(36, large_architecture(32), Vec::new());
    let mut handles = Vec::new();

    for _ in 0..50 {
        let state = state.clone();
        handles.push(thread::spawn(move || {
            DefaultSimulationEngine.simulate_with_trace(&state, None)
        }));
    }

    let results = handles
        .into_iter()
        .map(|handle| handle.join().expect("thread must complete"))
        .collect::<Vec<_>>();

    assert_eq!(results.len(), 50);
    assert!(results.iter().all(|result| result.traces.trace_complete));
}

#[test]
fn phase29_large_simulation_workload_remains_stable() {
    let state = WorldState::from_architecture(37, large_architecture(16), Vec::new());
    let mut last_total = None;

    for _ in 0..1000 {
        let traced = DefaultSimulationEngine.simulate_with_trace(&state, None);
        if let Some(previous) = last_total {
            assert_eq!(previous, traced.result.total());
        }
        last_total = Some(traced.result.total());
    }
}

#[test]
fn phase29_large_architecture_simulation_completes() {
    let state = WorldState::from_architecture(
        38,
        large_architecture(500),
        vec![Constraint {
            name: "large-graph-constraint".into(),
            max_design_units: Some(600),
            max_dependencies: Some(600),
        }],
    );

    let traced = DefaultSimulationEngine.simulate_with_trace(&state, None);

    assert_eq!(traced.traces.simulation_trace.step_count, 4);
    assert_eq!(traced.result.system.call_edges, 499);
    assert!(traced.result.constraint_score >= 0.0);
    assert!(traced.traces.trace_complete);
}
