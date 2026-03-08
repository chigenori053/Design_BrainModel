use runtime_vm::{ExecutionMode, HybridVm};

#[test]
fn pipeline_execution_and_context_propagation() {
    let mut vm = HybridVm::new(ExecutionMode::Reasoning);
    vm.set_input_text("optimize database query with cache");
    vm.execute();

    let ctx = vm.context();
    assert!(!ctx.semantic_units.is_empty());
    assert!(!ctx.concepts.is_empty());
    assert!(!ctx.intent_nodes.is_empty());
    assert!(ctx.search_state.is_some());
    assert!(ctx.hypothesis_graph.is_some());
    assert!(ctx.design_state.is_some());
}
