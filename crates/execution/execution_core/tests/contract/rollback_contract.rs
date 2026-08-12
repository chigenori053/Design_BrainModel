use code_ir::{CodeIr, DependencyIr, InterfaceIr, ModuleIr};
use design_domain::{DependencyKind, Layer};
use execution_core::{ExecutionContext, ExecutionInput, Executor, IrExecutor};

// IR that will fail validation: DependencyUpdate without a FileChange for module 99
fn invalid_ir() -> CodeIr {
    CodeIr {
        modules: vec![
            ModuleIr { id: 1, name: "Controller".into(), layer: Layer::Ui, responsibilities: vec![] },
        ],
        interfaces: vec![],
        dependencies: vec![
            // module 99 has no FileChange step (not in modules list)
            DependencyIr { from: 99, to: 1, kind: DependencyKind::Calls },
        ],
        control_flow: vec![],
        data_flow: vec![],
    }
}

#[test]
fn validation_failure_triggers_rollback() {
    let ctx = ExecutionContext { dry_run: false, max_steps: 64 };
    let result = IrExecutor.execute(ExecutionInput::with_context(invalid_ir(), ctx));

    assert!(
        !result.validation_result.success,
        "invalid IR must fail validation"
    );
    assert!(
        result.rollback_info.reverted,
        "failed validation must trigger rollback"
    );
    assert!(
        !result.rollback_info.reverted_changes.is_empty(),
        "rollback must record the reverted changes"
    );
    assert!(!result.success(), "overall result must be failure");
}

#[test]
fn dry_run_never_triggers_real_rollback() {
    // In dry-run mode nothing is applied, so rollback is always a no-op.
    let ctx = ExecutionContext { dry_run: true, max_steps: 64 };
    let result = IrExecutor.execute(ExecutionInput::with_context(invalid_ir(), ctx));

    assert!(
        !result.rollback_info.reverted,
        "dry-run mode must not trigger real rollback (nothing was applied)"
    );
    assert_eq!(result.rollback_info.steps_applied, 0);
}
