use execution_graph::{ExecutionEdgeType, ExecutionGraph, ExecutionNode};
use workload_model::{Distribution, WorkloadModel};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PerformanceEstimate {
    pub latency: f64,
    pub throughput: f64,
    pub cpu_usage: f64,
    pub predicted_queue_depth: usize,
}

#[derive(Clone, Debug, Default)]
pub struct PerformanceEstimator;

impl PerformanceEstimator {
    pub fn estimate(
        &self,
        graph: &ExecutionGraph,
        workload: &WorkloadModel,
    ) -> PerformanceEstimate {
        let sync_calls = graph
            .edges
            .iter()
            .filter(|edge| matches!(edge.edge_type, ExecutionEdgeType::SyncCall))
            .count() as f64;
        let async_edges = graph
            .edges
            .iter()
            .filter(|edge| {
                matches!(
                    edge.edge_type,
                    ExecutionEdgeType::AsyncMessage | ExecutionEdgeType::EventEmit
                )
            })
            .count() as f64;
        let data_accesses = graph
            .edges
            .iter()
            .filter(|edge| matches!(edge.edge_type, ExecutionEdgeType::DataAccess))
            .count() as f64;
        let queue_nodes = graph
            .nodes
            .iter()
            .filter(|node| matches!(node, ExecutionNode::Queue(_)))
            .count() as f64;
        let latency = 5.0
            + sync_calls * 3.0
            + async_edges * 1.5
            + data_accesses * 4.0
            + workload.request_rate / 50.0
            + workload.concurrency as f64 * 0.5;
        let throughput = (workload.request_rate * (1.0 - (data_accesses * 0.05).min(0.4))).max(1.0);
        let cpu_usage = (workload.request_rate / 120.0
            + workload.concurrency as f64 / 20.0
            + sync_calls * 0.08
            + async_edges * 0.05)
            .clamp(0.0, 1.0);
        let distribution_multiplier = match workload.request_distribution {
            Distribution::Uniform => 1.0,
            Distribution::Bursty => 1.4,
            Distribution::QueueHeavy => 2.0,
        };
        let predicted_queue_depth = ((queue_nodes * workload.request_rate / throughput.max(1.0))
            * distribution_multiplier)
            .round() as usize;

        PerformanceEstimate {
            latency,
            throughput,
            cpu_usage,
            predicted_queue_depth,
        }
    }
}
