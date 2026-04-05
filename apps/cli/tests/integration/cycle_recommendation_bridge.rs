use design_cli::commands::analyze::project::{
    AnalyzeMode, DecisionContext, DecisionMetrics, UnifiedAnalyzeResult,
};
use design_cli::dbm::analyzer::{Complexity, ProjectAnalysisResult, ProjectSummary};
use design_cli::viewer::{
    DesignSyncStatus, StructureViewIR, ViewEdge, ViewerSelection, inject_recommendation_candidates,
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

fn empty_analysis() -> ProjectAnalysisResult {
    ProjectAnalysisResult {
        files: Vec::new(),
        dependencies: Vec::new(),
        modules: Vec::new(),
        summary: ProjectSummary {
            total_files: 0,
            languages: Vec::new(),
            avg_complexity: Complexity::Low,
        },
    }
}

fn ir_with_edge(from: &str, to: &str) -> StructureViewIR {
    let mut ir = minimal_ir();
    ir.edges.push(ViewEdge {
        from: from.to_string(),
        to: to.to_string(),
        kind: "depends_on".to_string(),
        cycle: false,
    });
    ir
}

fn unified_result(action: &str, confidence: f64) -> UnifiedAnalyzeResult {
    UnifiedAnalyzeResult {
        path: ".".to_string(),
        mode: AnalyzeMode::Summary,
        intent: "balanced".to_string(),
        modules: 2,
        cycles: 1,
        coupling: "moderate".to_string(),
        top_issue: "cycle".to_string(),
        violations: Vec::new(),
        metrics: DecisionMetrics {
            si: 0.5,
            cs: 0.5,
            rp: 0.5,
            er: 0.5,
        },
        decision: DecisionContext {
            action: action.to_string(),
            expected_impact: "coupling down".to_string(),
            score: 0.8,
            confidence,
            risk: "Low".to_string(),
            intent_match: "balanced".to_string(),
        },
        analysis: empty_analysis(),
        report: None,
        design: None,
    }
}

#[test]
fn remove_dependency_action_injects_candidate() {
    let mut ir = minimal_ir();
    let analysis = unified_result("RemoveDependency(adapter -> world)", 0.9);
    inject_recommendation_candidates(&mut ir, &analysis);

    assert_eq!(
        ir.candidates.len(),
        1,
        "expected 1 candidate, got {:?}",
        ir.candidates.len()
    );
    let candidate = &ir.candidates[0];
    assert_eq!(candidate.from_node.logical_name, "adapter");
    assert_eq!(candidate.to_node.logical_name, "world");
}

#[test]
fn unrecognized_action_does_not_inject_candidates() {
    let mut ir = minimal_ir();
    let analysis = unified_result("random", 0.5);
    inject_recommendation_candidates(&mut ir, &analysis);

    assert!(
        ir.candidates.is_empty(),
        "expected empty candidates for unrecognized action"
    );
}

#[test]
fn matching_edge_gets_cycle_flag() {
    let mut ir = ir_with_edge("adapter", "world");
    let analysis = unified_result("RemoveDependency(adapter -> world)", 0.85);
    inject_recommendation_candidates(&mut ir, &analysis);

    let edge = ir
        .edges
        .iter()
        .find(|e| e.from == "adapter" && e.to == "world")
        .expect("edge not found");
    assert!(edge.cycle, "expected edge.cycle = true after injection");
}
