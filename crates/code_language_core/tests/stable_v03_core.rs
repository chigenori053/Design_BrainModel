use code_language_core::stable_v03::{
    CodeGenerator, CodeIRBuilder, DefaultCodeIRBuilder, PythonGenerator, RustGenerator,
    TypeScriptGenerator,
};
use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Edge, Node, NodeType, RelationType};
use unified_design_ir::{ArchitectureMapper, DefaultArchitectureMapper, DesignGraph};

fn design_graph() -> DesignGraph {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .add_edge(Edge::new("api", "service", RelationType::Calls))
        .build()
        .expect("valid graph");
    DefaultArchitectureMapper.map(&architecture)
}

#[test]
fn implementation_units_build_code_modules() {
    let units = design_graph().to_implementation_units();
    let modules = DefaultCodeIRBuilder.build(units);

    assert_eq!(modules.len(), 2);
    assert!(modules.iter().all(|module| !module.functions.is_empty()));
}

#[test]
fn generators_produce_deterministic_outputs() {
    let modules = DefaultCodeIRBuilder.build(design_graph().to_implementation_units());
    let rust = RustGenerator.generate(modules.clone());
    let python = PythonGenerator.generate(modules.clone());
    let ts = TypeScriptGenerator.generate(modules.clone());

    assert_eq!(rust, RustGenerator.generate(modules.clone()));
    assert_eq!(python, PythonGenerator.generate(modules.clone()));
    assert_eq!(ts, TypeScriptGenerator.generate(modules));
}

#[test]
fn rust_python_ts_outputs_match_expected_shapes() {
    let modules = DefaultCodeIRBuilder.build(design_graph().to_implementation_units());
    let rust = RustGenerator.generate(modules.clone());
    let python = PythonGenerator.generate(modules.clone());
    let ts = TypeScriptGenerator.generate(modules);

    assert!(rust[0].content.contains("pub fn"));
    assert!(python[0].content.contains("def "));
    assert!(ts[0].content.contains("export function"));
}
