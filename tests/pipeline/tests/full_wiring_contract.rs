use std::sync::Arc;

use design_search_engine::stable_v03::DeterministicBeamSearchEngine;
use integration_layer::{SystemInput, to_relations, to_system_output};
use memory_space_phase14::stable_v03::{InMemoryEngine, MemoryEngine};
use pipeline_tests::read_workspace_file;
use runtime_core::CoreRuntime;
use world_model::stable_v03::IntentInput;

#[test]
fn runtime_output_is_routed_through_integration_layer() {
    let runtime = CoreRuntime::new_with_defaults(
        Arc::new(InMemoryEngine::default()) as Arc<dyn MemoryEngine>,
        Arc::new(DeterministicBeamSearchEngine::default()),
    );
    let result = runtime
        .executor
        .execute(IntentInput::new("build rust api"))
        .expect("runtime succeeds");

    let expected_relations = to_relations(SystemInput::Architecture(result.architecture.clone()));
    let expected_output = to_system_output(expected_relations.clone());

    assert_eq!(result.output_relations, expected_relations);
    assert_eq!(result.system_output, expected_output);
    assert_eq!(result.trace_links.len(), result.output_relations.len());
}

#[test]
fn cli_and_runtime_sources_are_wired_via_integration_layer() {
    let cli_source = read_workspace_file("apps/cli/src/app.rs");
    let runtime_source = read_workspace_file("crates/runtime/runtime_core/src/stable_v03.rs");

    assert!(cli_source.contains("to_relations("));
    assert!(cli_source.contains("to_system_output("));
    assert!(runtime_source.contains("to_relations("));
    assert!(runtime_source.contains("to_system_output("));
}

#[test]
fn direct_braincore_calls_are_absent_from_wired_paths() {
    let cli_source = read_workspace_file("apps/cli/src/app.rs");
    let runtime_source = read_workspace_file("crates/runtime/runtime_core/src/stable_v03.rs");
    let loop_source = read_workspace_file("apps/cli/src/loop.rs");

    assert!(!cli_source.contains("brain_core"));
    assert!(!runtime_source.contains("brain_core"));
    assert!(!loop_source.contains("brain_core"));
}

#[test]
fn trace_links_preserve_relation_to_source_mapping() {
    let runtime = CoreRuntime::new_with_defaults(
        Arc::new(InMemoryEngine::default()) as Arc<dyn MemoryEngine>,
        Arc::new(DeterministicBeamSearchEngine::default()),
    );
    let result = runtime
        .executor
        .execute(IntentInput::new("build rust api"))
        .expect("runtime succeeds");

    for (trace_link, relation) in result
        .trace_links
        .iter()
        .zip(result.output_relations.iter())
    {
        assert_eq!(trace_link.relation_id, relation.id);
        assert_eq!(trace_link.provenance, relation.provenance);
    }
}
