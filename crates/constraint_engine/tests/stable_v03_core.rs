use std::sync::Arc;

use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Edge, Node, NodeType, RelationType};
use constraint_engine::stable_v03::{
    CompositeConstraintEngine, Constraint, ConstraintEngine, LayerOrderConstraint,
    NoCycleConstraint,
};
use design_search_engine::stable_v03::ArchitectureCandidate;

#[test]
fn cycle_graph_is_rejected() {
    let cyclic = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .add_edge(Edge::new("api", "service", RelationType::Calls))
        .add_edge(Edge::new("service", "api", RelationType::Calls))
        .build()
        .expect("valid graph");
    let engine =
        CompositeConstraintEngine::new(vec![Arc::new(NoCycleConstraint) as Arc<dyn Constraint>]);

    let filtered = engine.filter(vec![ArchitectureCandidate {
        id: "cyclic".to_string(),
        architecture: cyclic,
        score: 0.0,
        depth: 0,
    }]);

    assert!(filtered.is_empty());
}

#[test]
fn valid_graph_is_accepted() {
    let valid = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .add_node(Node::new("db", NodeType::DataStore))
        .add_edge(Edge::new("api", "service", RelationType::Calls))
        .add_edge(Edge::new("service", "db", RelationType::DependsOn))
        .build()
        .expect("valid graph");
    let engine = CompositeConstraintEngine::new(vec![
        Arc::new(NoCycleConstraint) as Arc<dyn Constraint>,
        Arc::new(LayerOrderConstraint) as Arc<dyn Constraint>,
    ]);

    let filtered = engine.filter(vec![ArchitectureCandidate {
        id: "valid".to_string(),
        architecture: valid,
        score: 0.0,
        depth: 0,
    }]);

    assert_eq!(filtered.len(), 1);
}
