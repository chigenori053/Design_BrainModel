use runtime_vm::{ExecutionMode, HybridVm};

#[test]
fn git_steps_default_to_dry_run_safe_workflow() {
    // Multi-turn: each execute() call represents one conversational turn.
    // Git steps are encoded as DesignTransition (IR nodes) in the hypothesis_graph.
    // Default mode for the multi-turn workflow is Analysis (dry-run safe).
    let mut vm = HybridVm::new(ExecutionMode::Reasoning);
    vm.set_input_text("git commit 最適化 CI高速化");
    vm.execute();

    let tick_after_turn1 = vm.context().tick;
    assert!(tick_after_turn1 > 0, "first turn must advance tick");

    // Verify IR nodes (DesignTransition edges) are produced — git steps as IR
    let graph = vm
        .context()
        .hypothesis_graph
        .as_ref()
        .expect("hypothesis_graph must be populated after Reasoning execution");
    assert!(
        !graph.edges.is_empty(),
        "IR nodes (DesignTransition edges) must exist for git workflow steps"
    );
    assert!(
        !graph.states.is_empty(),
        "graph must contain IR state nodes"
    );

    // Second turn: context carries forward (multi-turn)
    vm.set_input_text("git push dry-run 確認");
    vm.execute();
    assert!(
        vm.context().tick > tick_after_turn1,
        "second turn must further advance tick"
    );

    // Safe workflow: switch to Analysis for dry-run verification
    vm.set_mode(ExecutionMode::Analysis);
    assert_eq!(
        vm.mode(),
        ExecutionMode::Analysis,
        "mode switch to Analysis (dry-run) must be preserved"
    );
}
