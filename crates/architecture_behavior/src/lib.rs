use execution_graph::{ExecutionGraph, ExecutionNode};
use execution_simulator::{ExecutionSimulator, SimulationResult};
use performance_model::{PerformanceEstimate, PerformanceEstimator};
use workload_model::WorkloadModel;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BehaviorAnalysis {
    pub bottlenecks: Vec<u64>,
    pub hot_path: Vec<u64>,
    pub failure_propagation_risk: f64,
    pub behavior_score: f64,
    pub simulation: SimulationResult,
    pub performance: PerformanceEstimate,
}

#[derive(Clone, Debug, Default)]
pub struct BehaviorAnalyzer;

impl BehaviorAnalyzer {
    pub fn analyze(&self, graph: &ExecutionGraph, workload: &WorkloadModel) -> BehaviorAnalysis {
        let simulation = ExecutionSimulator.simulate(graph, workload);
        let performance = PerformanceEstimator.estimate(graph, workload);
        let hot_path = graph
            .nodes
            .iter()
            .filter_map(|node| match node {
                ExecutionNode::Component(id) => Some(*id),
                _ => None,
            })
            .take(3)
            .collect::<Vec<_>>();
        let failure_propagation_risk =
            ((graph.edges.len() as f64 / graph.nodes.len().max(1) as f64) + performance.cpu_usage)
                .clamp(0.0, 1.0)
                / 2.0;
        let behavior_score = ((1.0 - (performance.latency / 100.0).clamp(0.0, 1.0))
            + (performance.throughput / workload.request_rate.max(1.0)).clamp(0.0, 1.0)
            + (1.0 - failure_propagation_risk)
            + (1.0 - (simulation.bottlenecks.len() as f64 * 0.15).clamp(0.0, 1.0)))
            / 4.0;

        BehaviorAnalysis {
            bottlenecks: simulation.bottlenecks.clone(),
            hot_path,
            failure_propagation_risk,
            behavior_score: behavior_score.clamp(0.0, 1.0),
            simulation,
            performance,
        }
    }
}
