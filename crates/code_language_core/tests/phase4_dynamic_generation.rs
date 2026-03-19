use std::sync::Arc;

use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Edge, Node, NodeType, RelationType};
use code_language_core::stable_v03::{
    ContextualCodeIRBuilder, DefaultContextualCodeIRBuilder, DefaultGeneratorRegistry,
    DefaultProfileResolver, GeneratorRegistry, ProfileResolver, PythonTypeMapper, RustTypeMapper,
    TargetLanguage, TypeMapper, TypeScriptTypeMapper,
};
use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine, MemoryRecord};
use unified_design_ir::{ArchitectureMapper, DefaultArchitectureMapper, TypeRef};

fn implementation_unit() -> unified_design_ir::ImplementationUnit {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .add_edge(Edge::new("api", "service", RelationType::Calls))
        .build()
        .expect("valid graph");
    DefaultArchitectureMapper
        .map(&architecture)
        .to_implementation_units()
        .into_iter()
        .next()
        .expect("unit")
}

#[test]
fn profile_resolution_is_deterministic_for_same_memory() {
    let unit = implementation_unit();
    let memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    memory.store(MemoryRecord {
        id: "fastapi-api".to_string(),
        text: "api service python fastapi".to_string(),
        tags: vec!["lang:python".to_string(), "framework:fastapi".to_string()],
        embedding: None,
        architecture: None,
        relations: vec!["framework:fastapi".to_string()],
    });

    let first = DefaultProfileResolver.resolve(&unit, memory.as_ref());
    let second = DefaultProfileResolver.resolve(&unit, memory.as_ref());

    assert_eq!(first, second);
}

#[test]
fn type_mapping_works_for_rust_python_and_typescript() {
    let ty = TypeRef::Optional(Box::new(TypeRef::List(Box::new(TypeRef::Primitive(
        "string".to_string(),
    )))));

    assert_eq!(RustTypeMapper.map_type(&ty), "Option<Vec<String>>");
    assert_eq!(PythonTypeMapper.map_type(&ty), "Optional[list[str]]");
    assert_eq!(TypeScriptTypeMapper.map_type(&ty), "string[] | null");
}

#[test]
fn framework_profile_is_selected_from_memory() {
    let unit = implementation_unit();
    let memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    memory.store(MemoryRecord {
        id: "express-api".to_string(),
        text: "api express typescript".to_string(),
        tags: vec![
            "lang:typescript".to_string(),
            "framework:express".to_string(),
        ],
        embedding: None,
        architecture: None,
        relations: vec![],
    });

    let context = DefaultProfileResolver.resolve(&unit, memory.as_ref());

    assert_eq!(
        context.language_profile.language,
        TargetLanguage::TypeScript
    );
    assert_eq!(
        context
            .framework_profile
            .as_ref()
            .map(|profile| profile.name.as_str()),
        Some("express")
    );
}

#[test]
fn memory_changes_generation_context() {
    let unit = implementation_unit();
    let empty_memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    let framework_memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    framework_memory.store(MemoryRecord {
        id: "python-fastapi".to_string(),
        text: "api python fastapi".to_string(),
        tags: vec!["lang:python".to_string(), "framework:fastapi".to_string()],
        embedding: None,
        architecture: None,
        relations: vec![],
    });

    let without_memory = DefaultProfileResolver.resolve(&unit, empty_memory.as_ref());
    let with_memory = DefaultProfileResolver.resolve(&unit, framework_memory.as_ref());

    assert_ne!(without_memory, with_memory);
}

#[test]
fn generator_is_deterministic_given_same_context() {
    let unit = implementation_unit();
    let memory: Arc<dyn MemoryEngine> = Arc::new(InMemoryEngine::default());
    memory.store(MemoryRecord {
        id: "rust-axum".to_string(),
        text: "api rust axum".to_string(),
        tags: vec!["lang:rust".to_string(), "framework:axum".to_string()],
        embedding: None,
        architecture: None,
        relations: vec![],
    });
    let context = DefaultProfileResolver.resolve(&unit, memory.as_ref());
    let modules = DefaultContextualCodeIRBuilder.build_with_context(vec![(unit, context.clone())]);
    let generator = DefaultGeneratorRegistry.get_generator(&context);

    let first = generator.generate(modules.clone());
    let second = generator.generate(modules);

    assert_eq!(first, second);
}
