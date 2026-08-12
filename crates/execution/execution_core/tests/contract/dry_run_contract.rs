use code_ir::{CodeIr, DependencyIr, ModuleIr};
use design_domain::{DependencyKind, Layer};
use execution_core::{ExecutionContext, ExecutionInput, Executor, IrExecutor};

fn two_module_ir() -> CodeIr {
    CodeIr {
        modules: vec![
            ModuleIr { id: 1, name: "AuthService".into(), layer: Layer::Service, responsibilities: vec![] },
            ModuleIr { id: 2, name: "UserRepo".into(),    layer: Layer::Repository, responsibilities: vec![] },
        ],
        interfaces: vec![],
        dependencies: vec![DependencyIr { from: 1, to: 2, kind: DependencyKind::Reads }],
        control_flow: vec![],
        data_flow: vec![],
    }
}

#[test]
fn dry_run_default_produces_no_side_effects() {
    // Default ExecutionContext has dry_run = true.
    let result = IrExecutor.execute(ExecutionInput::new(two_module_ir()));

    assert!(result.dry_run, "default execution must be dry-run");
    // Changes are recorded as intent (observable) but not applied
    assert!(!result.applied_changes.is_empty(), "dry-run must still produce change records");
    // No real rollback required — nothing was written
    assert!(!result.rollback_info.reverted);
}

#[test]
fn dry_run_flag_is_propagated_to_result() {
    let ctx_dry  = ExecutionContext { dry_run: true,  max_steps: 64 };
    let ctx_live = ExecutionContext { dry_run: false, max_steps: 64 };

    let r_dry  = IrExecutor.execute(ExecutionInput::with_context(two_module_ir(), ctx_dry));
    let r_live = IrExecutor.execute(ExecutionInput::with_context(two_module_ir(), ctx_live));

    assert!(r_dry.dry_run,  "dry-run context must set result.dry_run = true");
    assert!(!r_live.dry_run, "live context must set result.dry_run = false");
}

#[test]
fn dry_run_change_count_equals_live_change_count() {
    // Dry-run must record the same steps as live execution (zero hidden changes).
    let ctx_dry  = ExecutionContext { dry_run: true,  max_steps: 64 };
    let ctx_live = ExecutionContext { dry_run: false, max_steps: 64 };

    let r_dry  = IrExecutor.execute(ExecutionInput::with_context(two_module_ir(), ctx_dry));
    let r_live = IrExecutor.execute(ExecutionInput::with_context(two_module_ir(), ctx_live));

    assert_eq!(
        r_dry.applied_changes.len(),
        r_live.applied_changes.len(),
        "dry-run must record identical step count as live execution"
    );
}
