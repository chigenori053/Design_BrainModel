use code_ir::{CodeIr, ControlFlowEdge, DataFlowEdge, DependencyIr, InterfaceDirection, InterfaceIr, ModuleIr};
use design_domain::{DependencyKind, Layer};
use execution_core::{ExecutionInput, Executor, IrExecutor, AppliedChange};

fn fixed_ir() -> CodeIr {
    CodeIr {
        modules: vec![
            ModuleIr { id: 10, name: "Gateway".into(),  layer: Layer::Ui,     responsibilities: vec!["routing".into()] },
            ModuleIr { id: 20, name: "Cache".into(),    layer: Layer::Repository,  responsibilities: vec!["caching".into()] },
            ModuleIr { id: 30, name: "Database".into(), layer: Layer::Repository,  responsibilities: vec!["persistence".into()] },
        ],
        interfaces: vec![
            InterfaceIr { module_id: 10, name: "Request".into(),  direction: InterfaceDirection::Input },
            InterfaceIr { module_id: 10, name: "Response".into(), direction: InterfaceDirection::Output },
        ],
        dependencies: vec![
            DependencyIr { from: 10, to: 20, kind: DependencyKind::Reads },
            DependencyIr { from: 10, to: 30, kind: DependencyKind::Calls },
        ],
        control_flow: vec![
            ControlFlowEdge { from: 10, to: 20, label: "cache-lookup".into() },
            ControlFlowEdge { from: 10, to: 30, label: "db-fallback".into() },
        ],
        data_flow: vec![
            DataFlowEdge { from: 10, to: 20, payload: "CacheKey".into() },
            DataFlowEdge { from: 10, to: 30, payload: "Query".into() },
        ],
    }
}

fn run() -> Vec<AppliedChange> {
    IrExecutor.execute(ExecutionInput::new(fixed_ir())).applied_changes
}

#[test]
fn same_ir_produces_identical_changes_across_10_runs() {
    let baseline = run();
    for i in 1..10 {
        let result = run();
        assert_eq!(
            baseline, result,
            "run {i}: execution must be fully deterministic — same IR must yield same changes"
        );
    }
}

#[test]
fn execution_step_descriptions_are_deterministic() {
    let a = run();
    let b = run();
    let descs_a: Vec<&str> = a.iter().map(|c| c.description.as_str()).collect();
    let descs_b: Vec<&str> = b.iter().map(|c| c.description.as_str()).collect();
    assert_eq!(descs_a, descs_b, "step descriptions must be deterministic");
}

#[test]
fn execution_step_order_matches_ir_declaration_order() {
    let changes = run();
    // First 3 steps must be module FileChanges in IR declaration order (ids 10, 20, 30)
    assert_eq!(changes[0].ir_module_id, 10);
    assert_eq!(changes[1].ir_module_id, 20);
    assert_eq!(changes[2].ir_module_id, 30);
}
