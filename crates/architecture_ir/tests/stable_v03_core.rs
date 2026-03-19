use architecture_ir::stable_v03::{
    ArchitectureGraphBuilder, ArchitectureQuery, Edge, Node, NodeId, NodeType, RelationType,
    ValidationError,
};

#[test]
fn builder_creates_valid_graph() {
    let graph = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Service))
        .add_node(Node::new("db", NodeType::DataStore))
        .add_edge(Edge::new("api", "db", RelationType::DependsOn))
        .build()
        .expect("graph should be valid");

    assert!(graph.validate().is_valid());
    assert_eq!(
        ArchitectureQuery::new(&graph)
            .nodes_by_type(&NodeType::Service)
            .len(),
        1
    );
}

#[test]
fn query_api_returns_neighbors_and_edges() {
    let graph = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Service))
        .add_node(Node::new("service", NodeType::Component))
        .add_node(Node::new("db", NodeType::DataStore))
        .add_edge(Edge::new("api", "service", RelationType::Calls))
        .add_edge(Edge::new("service", "db", RelationType::DependsOn))
        .build()
        .expect("graph should be valid");

    assert_eq!(
        graph.neighbors(NodeId::from("api")),
        vec![NodeId::from("service")]
    );
    assert_eq!(graph.find_by_type(NodeType::DataStore).len(), 1);
    assert_eq!(graph.outgoing(NodeId::from("service")).len(), 1);
    assert_eq!(graph.incoming(NodeId::from("service")).len(), 1);
}

#[test]
fn graph_is_immutable_after_clone() {
    let original = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Service))
        .build()
        .expect("graph should be valid");
    let cloned = original.clone();
    let updated = cloned.with_node(Node::new("db", NodeType::DataStore));

    assert_eq!(original.nodes().len(), 1);
    assert_eq!(updated.nodes().len(), 2);
}

#[test]
fn validation_rejects_missing_edge_endpoint() {
    let validation = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Service))
        .add_edge(Edge::new("api", "missing", RelationType::DependsOn))
        .build()
        .expect_err("graph should be invalid");

    assert!(
        validation
            .errors
            .contains(&ValidationError::MissingNode("missing".into()))
    );
}
