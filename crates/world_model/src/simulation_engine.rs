use memory_space_core::RecallResult;
use world_model_core::{
    BehaviorPredictionTrace, ConstraintValidationTrace, ExecutionModelMetrics,
    GeometryModelMetrics, MathModelMetrics, SimulationResult, SimulationTelemetryEvent,
    SimulationTelemetryEventKind, SimulationTrace, SimulationTraceBundle, StateTransitionTrace,
    SystemModelMetrics, WorldState,
};

use crate::{
    algebraic_stability, call_graph_edges, constraint_solver_score, dependency_cycle_count,
    estimate_dependency_cost, estimate_latency_score, estimate_memory_usage, execution_complexity,
    graph_layout_score, layout_balance_score, logic_verification_score, module_coupling_score,
    runtime_flow_score, spatial_constraint_score,
};

pub trait SimulationEngine {
    fn simulate(&self, state: &WorldState, recall: Option<&RecallResult>) -> SimulationResult;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultSimulationEngine;

#[derive(Debug, Clone, PartialEq)]
pub struct TracedSimulation {
    pub result: SimulationResult,
    pub telemetry_events: Vec<SimulationTelemetryEvent>,
    pub traces: SimulationTraceBundle,
}

impl DefaultSimulationEngine {
    pub fn simulate_with_trace(
        &self,
        state: &WorldState,
        recall: Option<&RecallResult>,
    ) -> TracedSimulation {
        let result = self.simulate(state, recall);
        let simulation_id = state.state_id;
        let step_count = 4;
        let mut telemetry_events = Vec::with_capacity(step_count + 2);
        telemetry_events.push(SimulationTelemetryEvent {
            simulation_id,
            kind: SimulationTelemetryEventKind::Started,
            step_index: None,
        });
        for step_index in 0..step_count {
            telemetry_events.push(SimulationTelemetryEvent {
                simulation_id,
                kind: SimulationTelemetryEventKind::Step,
                step_index: Some(step_index),
            });
        }
        telemetry_events.push(SimulationTelemetryEvent {
            simulation_id,
            kind: SimulationTelemetryEventKind::Completed,
            step_index: None,
        });

        TracedSimulation {
            traces: SimulationTraceBundle {
                simulation_trace: SimulationTrace {
                    simulation_id,
                    step_count,
                },
                state_transition_trace: StateTransitionTrace {
                    simulation_id,
                    transition_count: state.history.len() + 1,
                },
                constraint_validation_trace: ConstraintValidationTrace {
                    simulation_id,
                    validation_score: result.constraint_score,
                },
                behavior_prediction_trace: BehaviorPredictionTrace {
                    simulation_id,
                    predicted_score: result.correctness_score,
                },
                trace_complete: true,
            },
            result,
            telemetry_events,
        }
    }
}

impl SimulationEngine for DefaultSimulationEngine {
    fn simulate(&self, state: &WorldState, recall: Option<&RecallResult>) -> SimulationResult {
        let architecture = &state.architecture;
        let dependency_cycles = dependency_cycle_count(architecture);
        let module_coupling = module_coupling_score(architecture);
        let layering_score = runtime_flow_score(architecture);
        let call_edges = call_graph_edges(architecture);

        let system = SystemModelMetrics {
            dependency_cycles,
            module_coupling,
            layering_score,
            call_edges,
        };

        let math = MathModelMetrics {
            algebraic_score: algebraic_stability(architecture),
            logic_score: logic_verification_score(architecture),
            constraint_solver_score: constraint_solver_score(state),
        };

        let geometry = GeometryModelMetrics {
            graph_layout_score: graph_layout_score(architecture),
            layout_balance_score: layout_balance_score(architecture),
            spatial_constraint_score: spatial_constraint_score(state),
        };

        let execution = ExecutionModelMetrics {
            runtime_complexity: execution_complexity(architecture),
            memory_usage: estimate_memory_usage(architecture),
            dependency_cost: estimate_dependency_cost(architecture),
            latency_score: estimate_latency_score(architecture),
        };

        let recall_confidence = recall
            .and_then(|result| result.candidates.first())
            .map(|candidate| candidate.relevance_score)
            .unwrap_or(0.5);

        let performance_score =
            ((execution.latency_score + (1.0 - execution.dependency_cost)) / 2.0).clamp(0.0, 1.0);
        let correctness_score =
            ((math.logic_score + system.layering_score + geometry.graph_layout_score) / 3.0)
                .clamp(0.0, 1.0);
        let constraint_score = ((math.constraint_solver_score + geometry.spatial_constraint_score)
            / 2.0)
            .clamp(0.0, 1.0);
        let confidence_score =
            ((recall_confidence + math.algebraic_score + module_coupling) / 3.0).clamp(0.0, 1.0);

        SimulationResult {
            performance_score,
            correctness_score,
            constraint_score,
            confidence_score,
            system,
            math,
            geometry,
            execution,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use design_domain::Architecture;

    #[test]
    fn simulation_engine_returns_bounded_scores() {
        let state = WorldState::from_architecture(1, Architecture::seeded(), Vec::new());
        let result = DefaultSimulationEngine.simulate(&state, None);

        assert!((0.0..=1.0).contains(&result.performance_score));
        assert!((0.0..=1.0).contains(&result.correctness_score));
        assert!((0.0..=1.0).contains(&result.constraint_score));
        assert!((0.0..=1.0).contains(&result.total()));
    }

    #[test]
    fn traced_simulation_emits_started_step_completed_sequence() {
        let state = WorldState::from_architecture(1, Architecture::seeded(), Vec::new());
        let traced = DefaultSimulationEngine.simulate_with_trace(&state, None);

        assert_eq!(traced.telemetry_events.len(), 6);
        assert_eq!(
            traced.telemetry_events[0].kind,
            SimulationTelemetryEventKind::Started
        );
        assert_eq!(
            traced.telemetry_events.last().expect("completed").kind,
            SimulationTelemetryEventKind::Completed
        );
        assert_eq!(traced.traces.simulation_trace.step_count, 4);
        assert!(traced.traces.trace_complete);
    }
}
