use architecture_reasoner::ReverseArchitectureReasoner;
use code_ir::CodeIr;
use design_domain::DesignUnit;
use execution_graph::{ExecutionEdgeType, ExecutionGraphBuilder, ExecutionNode};

#[test]
fn test17_execution_graph_generation() {
    let mut gateway = DesignUnit::new(1, "ApiGateway");
    gateway.dependencies.push(design_domain::DesignUnitId(2));
    let mut service = DesignUnit::new(2, "WorkerService");
    service.dependencies.push(design_domain::DesignUnitId(3));
    service.dependencies.push(design_domain::DesignUnitId(4));
    let repository = DesignUnit::new(3, "UserRepository");
    let queue = DesignUnit::new(4, "EventQueue");
    let graph = ReverseArchitectureReasoner.infer_from_code_ir(&CodeIr::from_design_units(&[
        gateway, service, repository, queue,
    ]));

    let execution = ExecutionGraphBuilder.build(&graph);

    assert_eq!(execution.nodes.len(), 4);
    assert!(execution
        .nodes
        .iter()
        .any(|node| matches!(node, ExecutionNode::Queue(name) if name == "EventQueue")));
    assert!(execution
        .edges
        .iter()
        .any(|edge| matches!(edge.edge_type, ExecutionEdgeType::AsyncMessage)));
    assert!(execution
        .edges
        .iter()
        .any(|edge| matches!(edge.edge_type, ExecutionEdgeType::DataAccess)));
}
