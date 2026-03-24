use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Edge, Node, NodeType, RelationType};
use code_ir::program_v1::{
    BackendLanguage, BuildValidation, Effect, GenerationMode, Program, TargetDomain,
};
use design_domain::{Architecture, Dependency, DependencyKind, DesignUnit};
use unified_design_ir::{ArchitectureMapper, DefaultArchitectureMapper};

fn implementation_units() -> Vec<unified_design_ir::ImplementationUnit> {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .add_edge(Edge::new("api", "service", RelationType::Calls))
        .build()
        .expect("valid graph");
    let graph = DefaultArchitectureMapper.map(&architecture);
    graph.to_implementation_units()
}

#[test]
fn program_from_implementation_units_is_deterministic() {
    let units = implementation_units();
    let lhs =
        Program::from_implementation_units("Example", "1.0.0", vec![TargetDomain::Backend], &units);
    let rhs =
        Program::from_implementation_units("Example", "1.0.0", vec![TargetDomain::Backend], &units);
    assert_eq!(lhs, rhs);
    assert_eq!(lhs.modules.len(), 2);
    assert!(
        lhs.modules
            .iter()
            .all(|module| !module.functions.is_empty())
    );
}

#[test]
fn architecture_to_program_maps_dependencies() {
    let mut architecture = Architecture::default();
    let mut api = DesignUnit::new(1, "api");
    api.inputs.push("String".to_string());
    api.outputs.push("Bool".to_string());
    let mut service = DesignUnit::new(2, "service");
    service.outputs.push("String".to_string());
    architecture.add_design_unit(api);
    architecture.add_design_unit(service);
    architecture.dependencies.push(Dependency {
        from: design_domain::DesignUnitId(1),
        to: design_domain::DesignUnitId(2),
        kind: DependencyKind::Calls,
    });

    let program =
        Program::from_architecture("Example", "1.0.0", vec![TargetDomain::Cli], &architecture);

    assert_eq!(program.metadata.target_domains, vec![TargetDomain::Cli]);
    assert!(!program.dependencies.is_empty());
    assert!(program.modules.iter().any(|module| {
        module
            .functions
            .iter()
            .any(|function| function.effects.contains(&Effect::Mutation))
    }));
}

#[test]
fn backend_stub_rendering_matches_language_shapes() {
    let program = Program::from_implementation_units(
        "Example",
        "1.0.0",
        vec![TargetDomain::Backend],
        &implementation_units(),
    );

    let rust = program.render_stub_source_tree(BackendLanguage::Rust);
    let python = program.render_stub_source_tree(BackendLanguage::Python);
    let ts = program.render_stub_source_tree(BackendLanguage::TypeScript);

    assert!(
        rust.iter()
            .any(|(_, content)| content.contains("pub trait"))
    );
    assert!(python.iter().any(|(_, content)| content.contains("def ")));
    assert!(
        ts.iter()
            .any(|(_, content)| content.contains("export interface"))
    );
}

#[test]
fn generation_strategy_and_build_validation_defaults_are_safe() {
    let program = Program::new("Example");
    assert_eq!(program.generation_strategy.mode, GenerationMode::DryRun);
    assert!(program.generation_strategy.safety.backup);
    assert!(program.generation_strategy.safety.check);
    assert!(program.generation_strategy.safety.rollback_on_fail);
    assert_eq!(
        program.build_validation,
        BuildValidation {
            enabled: true,
            command: "cargo check".to_string(),
            sandbox: true,
        }
    );
}
