use execution_graph::{ExecutionEdge, ExecutionEdgeType, ExecutionGraph, ExecutionNode};
use performance_model::PerformanceEstimator;
use workload_model::{Distribution, WorkloadModel};

#[test]
fn test19_performance_estimation() {
    let graph = ExecutionGraph {
        nodes: vec![
            ExecutionNode::Component(1),
            ExecutionNode::Queue("Jobs".into()),
            ExecutionNode::Database("Analytics".into()),
        ],
        edges: vec![
            ExecutionEdge {
                source: 1,
                target: 2,
                edge_type: ExecutionEdgeType::AsyncMessage,
            },
            ExecutionEdge {
                source: 2,
                target: 3,
                edge_type: ExecutionEdgeType::DataAccess,
            },
        ],
    };
    let workload = WorkloadModel {
        request_rate: 120.0,
        concurrency: 32,
        request_distribution: Distribution::QueueHeavy,
    };

    let estimate = PerformanceEstimator.estimate(&graph, &workload);

    assert!(estimate.predicted_queue_depth > 0);
}
