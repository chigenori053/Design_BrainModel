use execution_graph::{ExecutionEdge, ExecutionEdgeType, ExecutionGraph, ExecutionNode};
use execution_simulator::ExecutionSimulator;
use workload_model::{Distribution, WorkloadModel};

#[test]
fn test18_workload_simulation() {
    let graph = ExecutionGraph {
        nodes: vec![
            ExecutionNode::Component(1),
            ExecutionNode::Component(2),
            ExecutionNode::Database("PrimaryDb".into()),
        ],
        edges: vec![
            ExecutionEdge {
                source: 1,
                target: 2,
                edge_type: ExecutionEdgeType::SyncCall,
            },
            ExecutionEdge {
                source: 2,
                target: 3,
                edge_type: ExecutionEdgeType::DataAccess,
            },
        ],
    };
    let workload = WorkloadModel {
        request_rate: 100.0,
        concurrency: 16,
        request_distribution: Distribution::Uniform,
    };

    let result = ExecutionSimulator.simulate(&graph, &workload);

    assert!(result.latency > 0.0);
}
