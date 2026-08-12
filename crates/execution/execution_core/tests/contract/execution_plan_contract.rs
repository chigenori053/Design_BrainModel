use code_ir::{CodeIr, ControlFlowEdge, DataFlowEdge, DependencyIr, InterfaceDirection, InterfaceIr, ModuleIr};
use design_domain::{DependencyKind, Layer};
use execution_core::{ExecutionContext, ExecutionInput, Executor, IrExecutor};

fn sample_ir() -> CodeIr {
    CodeIr {
        modules: vec![
            ModuleIr { id: 1, name: "ApiController".into(), layer: Layer::Ui, responsibilities: vec!["handle requests".into()] },
            ModuleIr { id: 2, name: "UserService".into(),   layer: Layer::Service,      responsibilities: vec!["user logic".into()] },
        ],
        interfaces: vec![
            InterfaceIr { module_id: 1, name: "HttpRequest".into(), direction: InterfaceDirection::Input },
            InterfaceIr { module_id: 1, name: "UserDto".into(),     direction: InterfaceDirection::Output },
        ],
        dependencies: vec![
            DependencyIr { from: 1, to: 2, kind: DependencyKind::Calls },
        ],
        control_flow: vec![
            ControlFlowEdge { from: 1, to: 2, label: "calls".into() },
        ],
        data_flow: vec![
            DataFlowEdge { from: 1, to: 2, payload: "UserDto".into() },
        ],
    }
}

#[test]
fn ir_plan_maps_1to1_to_execution_steps() {
    let plan = sample_ir();
    // Expected steps: 2 modules + 2 interfaces + 1 dep + 1 control + 1 data = 7
    let expected_step_count = plan.modules.len()
        + plan.interfaces.len()
        + plan.dependencies.len()
        + plan.control_flow.len()
        + plan.data_flow.len();

    let result = IrExecutor.execute(ExecutionInput::new(plan));

    assert_eq!(
        result.applied_changes.len(),
        expected_step_count,
        "each IR element must map to exactly one ExecutionStep (1:1 traceability)"
    );
    // Step IDs must be sequential and unique
    let ids: Vec<usize> = result.applied_changes.iter().map(|c| c.step_id).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(ids.len(), sorted.len(), "step IDs must be unique");
}

#[test]
fn execution_result_schema_is_complete() {
    let result = IrExecutor.execute(ExecutionInput::new(sample_ir()));

    // All three output fields must be present
    assert!(!result.applied_changes.is_empty());
    assert!(result.validation_result.success);
    assert!(!result.rollback_info.reverted);
    assert!(result.success());
}

#[test]
fn max_steps_bounds_execution() {
    let plan = sample_ir();
    let ctx = ExecutionContext { dry_run: true, max_steps: 2 };
    let result = IrExecutor.execute(ExecutionInput::with_context(plan, ctx));

    assert!(
        result.applied_changes.len() <= 2,
        "max_steps must bound execution to at most 2 changes"
    );
}
