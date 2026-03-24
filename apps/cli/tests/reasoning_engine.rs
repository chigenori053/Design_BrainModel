use std::path::{Path, PathBuf};

use design_cli::service::{
    RootCause, analyze_path, design_graph_from_analysis, enrich_analysis_report, generate_plan,
    infer_root_cause,
};
use integration_layer::{Evidence, EvidenceType, Issue, IssueScope, IssueType, Severity};

fn fixture_root(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn make_issue(kind: IssueType, severity: Severity, scope: IssueScope) -> Issue {
    Issue {
        id: format!("{kind:?}-{scope:?}"),
        kind,
        severity,
        scope,
        description: "fixture".to_string(),
        evidence: vec![Evidence {
            kind: EvidenceType::Pattern,
            value: "fixture".to_string(),
        }],
    }
}

#[test]
fn test_root_cause_inference() {
    let issues = vec![
        make_issue(
            IssueType::Cycle,
            Severity::Critical,
            IssueScope::Subgraph(vec!["renderer".to_string(), "world".to_string()]),
        ),
        make_issue(
            IssueType::LayerViolation,
            Severity::High,
            IssueScope::Edge("world".to_string(), "renderer".to_string()),
        ),
        make_issue(
            IssueType::RoleMismatch,
            Severity::Medium,
            IssueScope::Edge("renderer".to_string(), "world".to_string()),
        ),
    ];

    let root = infer_root_cause(&issues);
    assert_eq!(root.label, "Layer Collapse");
    assert_eq!(root.confidence, 0.92);
}

#[test]
fn test_refactor_plan() {
    let root = RootCause {
        label: "Layer Collapse".to_string(),
        confidence: 1.0,
    };

    let plan = generate_plan(&root);

    assert!(!plan.is_empty());
    assert_eq!(plan[0].description, "Introduce service layer");
}

#[test]
fn test_end_to_end() {
    let path = fixture_root("architecture_layer_collapse");
    let analysis = analyze_path(&path).expect("fixture analysis");
    let graph = design_graph_from_analysis(&analysis);
    let report = enrich_analysis_report(analysis, integration_layer::diagnostic_analysis(&graph));

    let root = report.root_cause.expect("root cause");
    let plan = report.refactor_plan;

    assert_eq!(root.label, "Layer Collapse");
    assert!(
        plan.iter()
            .any(|step| step.description == "Introduce service layer")
    );
}

#[test]
fn test_reasoning_is_deterministic() {
    let path = fixture_root("architecture_layer_collapse");

    let analyze_once = || {
        let analysis = analyze_path(&path).expect("fixture analysis");
        let graph = design_graph_from_analysis(&analysis);
        enrich_analysis_report(analysis, integration_layer::diagnostic_analysis(&graph))
    };

    let first = analyze_once();
    let second = analyze_once();

    assert_eq!(first.root_cause, second.root_cause);
    assert_eq!(first.refactor_plan, second.refactor_plan);
}
