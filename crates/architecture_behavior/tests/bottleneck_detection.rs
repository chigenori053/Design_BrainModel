use architecture_behavior::BehaviorAnalyzer;
use execution_graph::{ExecutionEdge, ExecutionEdgeType, ExecutionGraph, ExecutionNode};
use workload_model::{Distribution, WorkloadModel};

#[test]
fn test20_bottleneck_detection() {
    let graph = ExecutionGraph {
        nodes: vec![
            ExecutionNode::Component(1),
            ExecutionNode::Database("PrimaryDb".into()),
            ExecutionNode::Queue("Events".into()),
        ],
        edges: vec![
            ExecutionEdge {
                source: 1,
                target: 2,
                edge_type: ExecutionEdgeType::DataAccess,
            },
            ExecutionEdge {
                source: 1,
                target: 3,
                edge_type: ExecutionEdgeType::EventEmit,
            },
        ],
    };
    let workload = WorkloadModel {
        request_rate: 200.0,
        concurrency: 64,
        request_distribution: Distribution::QueueHeavy,
    };

    let analysis = BehaviorAnalyzer.analyze(&graph, &workload);

    assert!(!analysis.bottlenecks.is_empty());
    assert!(analysis.simulation.queue_depth > 0);
}
