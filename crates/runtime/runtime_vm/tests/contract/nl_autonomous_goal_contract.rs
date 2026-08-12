use runtime_vm::{ExecutionMode, HybridVm};

#[test]
fn max_iteration_stop_works() {
    // Simulation pipeline uses ReasoningRuntimeAgent::new(16) — max 16 pairs.
    // Verify hypotheses are bounded and execution terminates deterministically.
    let mut vm = HybridVm::new(ExecutionMode::Simulation);
    vm.set_input_text("高速化 クラウド非依存 低メモリ");
    vm.execute();

    let ctx = vm.context();
    assert!(ctx.tick > 0, "pipeline must advance tick");
    // Simulation mode uses ReasoningRuntimeAgent(16): bounded output
    assert!(
        ctx.hypotheses.len() <= 16,
        "hypothesis count must be bounded by max_pairs=16, got {}",
        ctx.hypotheses.len()
    );
}

#[test]
fn safe_defaults_and_git_dry_run_are_preserved() {
    // Analysis mode is the IR-first dry-run path: no design mutations applied.
    // design_state and hypothesis_graph remain None — read-only execution.
    let mut vm = HybridVm::new(ExecutionMode::Analysis);
    vm.set_input_text("セキュリティ強化 認証基盤刷新");
    vm.execute();

    assert_eq!(
        vm.mode(),
        ExecutionMode::Analysis,
        "default mode must remain Analysis (dry-run safe)"
    );
    let ctx = vm.context();
    assert!(
        ctx.design_state.is_none(),
        "Analysis mode must not mutate design state (git dry-run equivalent)"
    );
    assert!(
        ctx.hypothesis_graph.is_none(),
        "Analysis mode must not produce hypothesis graph"
    );
    assert!(!ctx.semantic_units.is_empty(), "semantic parsing must run");
    assert!(!ctx.concepts.is_empty(), "concept extraction must run");
}
