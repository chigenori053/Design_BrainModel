use std::path::PathBuf;

use design_cli::refactor::{
    ApplyPreviewPlan, RefactorActionKind, RefactorCandidate, RefactorOperation, RefactorTarget,
    RollbackPreview, StructureEdge,
};
use design_cli::service::ModuleNode;
use design_cli::source_index::QualifiedModuleId;
use design_cli::viewer::{
    CameraPreset3D, DesignSyncStatus, Node3D, SemanticGraph3D, SourceBinding, Structure3DIr,
    StructureViewIR, Vec3, ViewerOverlays3D, ViewerSelection, sync_apply_preview_with_selection,
    sync_preview_with_selection,
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
        scene_3d: Some(Structure3DIr {
            graph: SemanticGraph3D {
                nodes: vec![Node3D {
                    id: "adapter".to_string(),
                    label: "adapter".to_string(),
                    kind: "module".to_string(),
                    position: Vec3::default(),
                    size: 1.0,
                    importance: 1.0,
                    heat: 0.0,
                    source_binding: Some(SourceBinding {
                        file: PathBuf::from("crates/runtime/runtime_vm/src/adapter.rs"),
                        line_start: 1,
                        line_end: 1,
                        symbol: Some("adapter".to_string()),
                    }),
                }],
                ..SemanticGraph3D::default()
            },
            overlays: ViewerOverlays3D::default(),
            camera: CameraPreset3D::default(),
            ..Structure3DIr::default()
        }),
    }
}

fn remove_dependency_candidate() -> RefactorCandidate {
    let adapter = QualifiedModuleId {
        crate_name: String::new(),
        module_path: "adapter".to_string(),
    };
    let world = QualifiedModuleId {
        crate_name: String::new(),
        module_path: "world".to_string(),
    };
    RefactorCandidate {
        candidate_id: "cut-adapter-world".to_string(),
        module_id: adapter.clone(),
        logical_name: "adapter".to_string(),
        kind: RefactorActionKind::RemoveDependency,
        operation: RefactorOperation::RemoveDependency,
        title: "Remove dependency adapter -> world".to_string(),
        rationale: "coupling down".to_string(),
        confidence_milli: 900,
        confidence: 0.9,
        from_node: ModuleNode {
            qualified_id: adapter,
            logical_name: "adapter".to_string(),
            source_path: None,
        },
        to_node: ModuleNode {
            qualified_id: world,
            logical_name: "world".to_string(),
            source_path: None,
        },
        patch_plan: RefactorTarget::RemoveDependency {
            from: "adapter".to_string(),
            to: "world".to_string(),
        },
        source_path: PathBuf::new(),
        preview_hash: String::new(),
        base_file_hash: String::new(),
        target_nodes: vec!["adapter".to_string(), "world".to_string()],
        target_edges: vec![StructureEdge {
            from: "adapter".to_string(),
            to: "world".to_string(),
        }],
        target: RefactorTarget::RemoveDependency {
            from: "adapter".to_string(),
            to: "world".to_string(),
        },
    }
}

#[test]
fn apply_preview_bridge_generates_plan_for_selected_candidate() {
    let mut ir = minimal_ir();
    ir.candidates.push(remove_dependency_candidate());
    ir.selection = ViewerSelection {
        selected_nodes: Vec::new(),
        selected_edges: vec![StructureEdge {
            from: "adapter".to_string(),
            to: "world".to_string(),
        }],
        selection_mode: "single".to_string(),
    };

    sync_preview_with_selection(&mut ir);
    sync_apply_preview_with_selection(&mut ir);

    let plan = ir.apply_preview.expect("apply preview");
    assert!(!plan.write);
    assert_eq!(
        plan,
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
    );
}

#[test]
fn apply_preview_bridge_is_none_without_preview() {
    let mut ir = minimal_ir();
    ir.candidates.push(remove_dependency_candidate());

    sync_apply_preview_with_selection(&mut ir);

    assert!(ir.apply_preview.is_none());
}

#[test]
fn apply_preview_bridge_resolves_target_file_from_source_binding() {
    let mut ir = minimal_ir();
    ir.candidates.push(remove_dependency_candidate());
    ir.selection = ViewerSelection {
        selected_nodes: vec!["adapter".to_string()],
        selected_edges: Vec::new(),
        selection_mode: "single".to_string(),
    };

    sync_preview_with_selection(&mut ir);
    sync_apply_preview_with_selection(&mut ir);

    let plan = ir.apply_preview.expect("apply preview");
    assert_eq!(
        plan.target_files[0],
        "crates/runtime/runtime_vm/src/adapter.rs"
    );
}
