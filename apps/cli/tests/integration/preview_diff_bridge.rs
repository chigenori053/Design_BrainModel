use design_cli::refactor::{
    PreviewDiff, RefactorActionKind, RefactorCandidate, RefactorOperation, RefactorTarget,
    StructureEdge,
};
use design_cli::service::ModuleNode;
use design_cli::source_index::QualifiedModuleId;
use design_cli::viewer::{
    DesignSyncStatus, StructureViewIR, ViewerSelection, resolve_selected_candidate,
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
        scene_3d: None,
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
        source_path: std::path::PathBuf::new(),
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
fn preview_diff_resolves_from_selected_edge() {
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

    let preview = ir.preview.expect("preview");
    assert!(preview.safe);
    assert_eq!(
        preview,
        PreviewDiff {
            candidate_id: "cut-adapter-world".to_string(),
            summary: "Remove dependency adapter -> world".to_string(),
            estimated_effect: "coupling down".to_string(),
            safe: true,
            diff_lines: vec![
                "- adapter -> world".to_string(),
                "+ adapter -> ports".to_string(),
            ],
        }
    );
}

#[test]
fn preview_diff_falls_back_to_first_candidate_when_selection_is_empty() {
    let mut ir = minimal_ir();
    ir.candidates.push(remove_dependency_candidate());

    sync_preview_with_selection(&mut ir);

    assert_eq!(
        ir.preview
            .as_ref()
            .map(|preview| preview.candidate_id.as_str()),
        Some("cut-adapter-world")
    );
}

#[test]
fn preview_diff_is_none_when_no_candidate_exists() {
    let mut ir = minimal_ir();

    sync_preview_with_selection(&mut ir);

    assert!(resolve_selected_candidate(&ir).is_none());
    assert!(ir.preview.is_none());
}
