use execution_graph::{ExecutionEdgeType, ExecutionGraph, ExecutionNode};
use performance_model::{PerformanceEstimate, PerformanceEstimator};
use workload_model::WorkloadModel;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SimulationResult {
    pub latency: f64,
    pub queue_depth: usize,
    pub bottlenecks: Vec<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct ExecutionSimulator;

impl ExecutionSimulator {
    pub fn simulate(&self, graph: &ExecutionGraph, workload: &WorkloadModel) -> SimulationResult {
        let estimate = PerformanceEstimator.estimate(graph, workload);
        SimulationResult {
            latency: estimate.latency,
            queue_depth: estimate.predicted_queue_depth,
            bottlenecks: detect_bottlenecks(graph, &estimate),
        }
    }
}

fn detect_bottlenecks(graph: &ExecutionGraph, estimate: &PerformanceEstimate) -> Vec<u64> {
    let mut bottlenecks = graph
        .nodes
        .iter()
        .filter_map(|node| match node {
            ExecutionNode::Component(id) => {
                let outgoing = graph.edges.iter().filter(|edge| edge.source == *id).count();
                if outgoing >= 2 || estimate.cpu_usage > 0.75 {
                    Some(*id)
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if bottlenecks.is_empty() {
        if let Some(first_component) = graph.nodes.iter().find_map(|node| match node {
            ExecutionNode::Component(id) => Some(*id),
            _ => None,
        }) {
            bottlenecks.push(first_component);
        }
    }
    if graph
        .edges
        .iter()
        .any(|edge| matches!(edge.edge_type, ExecutionEdgeType::DataAccess))
        && estimate.predicted_queue_depth > 0
    {
        let database_proxy = 10_000
            + graph
                .nodes
                .iter()
                .filter(|node| matches!(node, ExecutionNode::Database(_)))
                .count() as u64;
        bottlenecks.push(database_proxy);
    }
    bottlenecks.sort_unstable();
    bottlenecks.dedup();
    bottlenecks
}
