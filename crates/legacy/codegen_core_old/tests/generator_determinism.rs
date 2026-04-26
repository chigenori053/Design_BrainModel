use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Edge, Node, NodeType, RelationType};
use code_language_core::stable_v03::{
    ContextualCodeIRBuilder, DefaultContextualCodeIRBuilder, DefaultGeneratorRegistry,
    GeneratorRegistry, TargetLanguage, default_generation_context,
};
use unified_design_ir::{ArchitectureMapper, DefaultArchitectureMapper};

fn fixed_module() -> (
    Vec<code_language_core::stable_v03::SpecializedCodeModule>,
    code_language_core::stable_v03::GenerationContext,
) {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("billing_api", NodeType::Interface))
        .add_node(Node::new("billing_service", NodeType::Service))
        .add_edge(Edge::new(
            "billing_api",
            "billing_service",
            RelationType::Calls,
        ))
        .build()
        .expect("valid graph");
    let unit = DefaultArchitectureMapper
        .map(&architecture)
        .to_implementation_units()
        .into_iter()
        .next()
        .expect("unit");
    let context = default_generation_context(TargetLanguage::Rust, None);
    let modules = DefaultContextualCodeIRBuilder.build_with_context(vec![(unit, context.clone())]);
    (modules, context)
}

#[test]
fn generator_is_pure_function() {
    let (modules, context) = fixed_module();
    let generator = DefaultGeneratorRegistry.get_generator(&context);

    let files1 = generator.generate(modules.clone());
    let files2 = generator.generate(modules);

    assert_eq!(files1, files2);
}
