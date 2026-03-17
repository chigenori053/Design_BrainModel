use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Edge, Node, NodeType, RelationType};
use unified_design_ir::{
    ArchitectureMapper, DefaultArchitectureMapper, DefaultDesignValidator, DesignNodeKind,
    DesignQuery, DesignValidator,
};

fn architecture_graph() -> architecture_ir::stable_v03::ArchitectureGraph {
    ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .add_node(Node::new("db", NodeType::DataStore))
        .add_edge(Edge::new("api", "service", RelationType::Calls))
        .add_edge(Edge::new("service", "db", RelationType::DependsOn))
        .build()
        .expect("valid graph")
}

#[test]
fn maps_architecture_to_udir() {
    let design = DefaultArchitectureMapper.map(&architecture_graph());
    assert_eq!(design.nodes().len(), 3);
    assert_eq!(design.edges().len(), 2);
}

#[test]
fn query_finds_kind_and_dependencies() {
    let design = DefaultArchitectureMapper.map(&architecture_graph());
    let query = DesignQuery::new(&design);
    assert_eq!(query.find_by_kind(DesignNodeKind::API).len(), 1);
    assert_eq!(query.dependencies("api".into()), vec!["service".into()]);
}

#[test]
fn validator_rejects_invalid_graph() {
    let invalid = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .add_node(Node::new("db", NodeType::DataStore))
        .build()
        .expect("valid graph");
    let design = DefaultArchitectureMapper.map(&invalid);
    assert!(!DefaultDesignValidator.validate(&design));
}

#[test]
fn implementation_units_are_derived() {
    let design = DefaultArchitectureMapper.map(&architecture_graph());
    let units = design.to_implementation_units();
    assert_eq!(units.len(), 3);
    assert!(units.iter().any(|unit| unit.module_name == "api"));
    assert_eq!(units[0].public_interfaces.len(), 1);
    assert_eq!(units[0].internal_structs.len(), 1);
}
