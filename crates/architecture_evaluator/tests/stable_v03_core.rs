use architecture_evaluator::stable_v03::{ArchitectureEvaluator, WeightedArchitectureEvaluator};
use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Edge, Node, NodeType, RelationType};

#[test]
fn simple_graph_scores_higher_than_complex_graph() {
    let simple = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .add_edge(Edge::new("api", "service", RelationType::Calls))
        .build()
        .expect("valid graph");
    let complex = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .add_node(Node::new("worker", NodeType::Component))
        .add_node(Node::new("db", NodeType::DataStore))
        .add_edge(Edge::new("api", "service", RelationType::Calls))
        .add_edge(Edge::new("service", "worker", RelationType::DependsOn))
        .add_edge(Edge::new("worker", "db", RelationType::Writes))
        .add_edge(Edge::new("db", "service", RelationType::Reads))
        .build()
        .expect("valid graph");
    let evaluator = WeightedArchitectureEvaluator::default();

    let simple_score = evaluator.evaluate(&simple);
    let complex_score = evaluator.evaluate(&complex);

    assert!(simple_score.score > complex_score.score);
}

#[test]
fn evaluation_is_deterministic() {
    let graph = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .add_edge(Edge::new("api", "service", RelationType::Calls))
        .build()
        .expect("valid graph");
    let evaluator = WeightedArchitectureEvaluator::default();

    let first = evaluator.evaluate(&graph);
    let second = evaluator.evaluate(&graph);

    assert_eq!(first, second);
}
