use design_cli::refactor::{
    ApplyPreviewPlan, RollbackPreview, SandboxWritePreview, TransactionExecutionPreview,
    TransactionPreview, TransactionRollbackPreview,
};
use design_cli::viewer::{
    DesignSyncStatus, StructureViewIR, ViewerSelection, sync_transaction_execution_with_selection,
};

fn minimal_ir() -> StructureViewIR {
    StructureViewIR {
        version: 2,
        nodes: Vec::new(),
        edges: Vec::new(),
        preview: None,
        apply_preview: None,
        transaction_preview: None,
        transaction_execution: None,
        transaction_result: None,
        promote_result: None,
        git_commit_preview: None,
        snapshots: Vec::new(),
        history: Vec::new(),
        risk_overlay: Vec::new(),
        selection: ViewerSelection::default(),
        candidates: Vec::new(),
        heatmap: Vec::new(),
        design_sync: DesignSyncStatus::default(),
        scene_3d: None,
    }
}

fn apply_preview_plan() -> ApplyPreviewPlan {
    ApplyPreviewPlan {
        candidate_id: "cut-adapter-world".to_string(),
        target_files: vec!["crates/runtime/runtime_vm/src/adapter.rs".to_string()],
        operations: vec!["RemoveDependency(adapter -> world)".to_string()],
        checks: vec!["cargo check -p runtime_vm".to_string()],
        rollback: RollbackPreview {
            mode: "git diff based".to_string(),
            safe: true,
        },
        write: false,
    }
}

fn transaction_preview_allowed() -> TransactionPreview {
    TransactionPreview {
        candidate_id: "cut-adapter-world".to_string(),
        allowed: true,
        safe: true,
        steps: vec![
            "write patch to adapter.rs".to_string(),
            "cargo check -p runtime_vm".to_string(),
            "rollback on failure".to_string(),
        ],
        rollback_strategy: TransactionRollbackPreview {
            mode: "transactional git diff".to_string(),
            guaranteed: true,
        },
        write: false,
    }
}

#[test]
fn transaction_execution_bridge_generates_execution_preview() {
    let mut ir = minimal_ir();
    ir.apply_preview = Some(apply_preview_plan());
    ir.transaction_preview = Some(transaction_preview_allowed());

    sync_transaction_execution_with_selection(&mut ir);

    let exec = ir.transaction_execution.expect("transaction execution");
    assert!(!exec.executed);
    assert!(!exec.write);
    assert_eq!(
        exec,
        TransactionExecutionPreview {
            candidate_id: "cut-adapter-world".to_string(),
            allowed: true,
            executed: false,
            sandbox_write: SandboxWritePreview {
                enabled: true,
                target_files: vec!["crates/runtime/runtime_vm/src/adapter.rs".to_string()],
            },
            steps: vec![
                "sandbox patch write".to_string(),
                "cargo check -p runtime_vm".to_string(),
                "commit preview".to_string(),
                "rollback on fail".to_string(),
            ],
            rollback_guaranteed: true,
            write: false,
        }
    );
}

#[test]
fn transaction_execution_bridge_is_none_when_tx_not_allowed() {
    let mut ir = minimal_ir();
    ir.apply_preview = Some(apply_preview_plan());
    let mut tx = transaction_preview_allowed();
    tx.allowed = false;
    ir.transaction_preview = Some(tx);

    sync_transaction_execution_with_selection(&mut ir);

    assert!(ir.transaction_execution.is_none());
}

#[test]
fn transaction_execution_bridge_exposes_sandbox_target_files() {
    let mut ir = minimal_ir();
    ir.apply_preview = Some(apply_preview_plan());
    ir.transaction_preview = Some(transaction_preview_allowed());

    sync_transaction_execution_with_selection(&mut ir);

    let exec = ir.transaction_execution.expect("transaction execution");
    assert_eq!(
        exec.sandbox_write.target_files[0],
        "crates/runtime/runtime_vm/src/adapter.rs"
    );
}
