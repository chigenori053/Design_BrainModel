use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Edge, Node, NodeType, RelationType};
use code_language_core::stable_v03::{
    ContextualCodeIRBuilder, DefaultContextualCodeIRBuilder, TargetLanguage,
    default_generation_context,
};
use unified_design_ir::{ArchitectureMapper, DefaultArchitectureMapper};

#[derive(Debug, PartialEq, Eq)]
struct StructureModel {
    interface_count: usize,
    interface_names: Vec<String>,
    function_count: usize,
    dependency_count: usize,
}

fn sample_unit() -> unified_design_ir::ImplementationUnit {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("user_api", NodeType::Interface))
        .add_node(Node::new("user_service", NodeType::Service))
        .add_edge(Edge::new("user_api", "user_service", RelationType::Calls))
        .build()
        .expect("valid graph");
    DefaultArchitectureMapper
        .map(&architecture)
        .to_implementation_units()
        .into_iter()
        .next()
        .expect("unit")
}

fn normalize(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn structure_for(language: TargetLanguage) -> StructureModel {
    let unit = sample_unit();
    let modules = DefaultContextualCodeIRBuilder
        .build_with_context(vec![(unit, default_generation_context(language, None))]);
    let module = &modules[0];
    let mut interface_names = module
        .interfaces
        .iter()
        .map(|interface| normalize(&interface.name))
        .collect::<Vec<_>>();
    interface_names.sort();
    StructureModel {
        interface_count: module.interfaces.len(),
        interface_names,
        function_count: module.functions.len(),
        dependency_count: module.imports.len(),
    }
}

#[test]
fn cross_language_structure_equivalence() {
    let rust = structure_for(TargetLanguage::Rust);
    let python = structure_for(TargetLanguage::Python);
    let typescript = structure_for(TargetLanguage::TypeScript);

    assert_eq!(rust, python);
    assert_eq!(python, typescript);
}
