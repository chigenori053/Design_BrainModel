use design_cli::refactor::{
    ApplyPreviewPlan, RollbackPreview, TransactionPreview, TransactionRollbackPreview,
};
use design_cli::viewer::{
    DesignSyncStatus, StructureViewIR, ViewerSelection, sync_transaction_preview_with_selection,
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

#[test]
fn transaction_preview_bridge_generates_transaction_gate() {
    let mut ir = minimal_ir();
    ir.apply_preview = Some(apply_preview_plan());

    sync_transaction_preview_with_selection(&mut ir);

    let tx = ir.transaction_preview.expect("transaction preview");
    assert!(tx.allowed);
    assert!(!tx.write);
    assert_eq!(
        tx,
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
    );
}

#[test]
fn transaction_preview_bridge_is_none_without_apply_preview() {
    let mut ir = minimal_ir();

    sync_transaction_preview_with_selection(&mut ir);

    assert!(ir.transaction_preview.is_none());
}

#[test]
fn transaction_preview_bridge_disallows_invalid_plan() {
    let mut ir = minimal_ir();
    let mut plan = apply_preview_plan();
    plan.target_files.clear();
    ir.apply_preview = Some(plan);

    sync_transaction_preview_with_selection(&mut ir);

    let tx = ir.transaction_preview.expect("transaction preview");
    assert!(!tx.allowed);
    assert!(!tx.write);
}
