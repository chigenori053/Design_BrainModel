use integration_layer::{Issue, IssueType};

use crate::service::dto::{RefactorStep, RootCause};

pub trait IssueAggregator {
    fn infer_root_cause(issues: Vec<Issue>) -> RootCause;
}

pub struct DeterministicIssueAggregator;

impl IssueAggregator for DeterministicIssueAggregator {
    fn infer_root_cause(issues: Vec<Issue>) -> RootCause {
        infer_root_cause(&issues)
    }
}

pub fn infer_root_cause(issues: &[Issue]) -> RootCause {
    let has_cycle = issues.iter().any(|issue| issue.kind == IssueType::Cycle);
    let has_layer_violation = issues
        .iter()
        .any(|issue| issue.kind == IssueType::LayerViolation);
    let has_role_mismatch = issues
        .iter()
        .any(|issue| issue.kind == IssueType::RoleMismatch);
    let has_data_flow_anomaly = issues
        .iter()
        .any(|issue| issue.kind == IssueType::DataFlowAnomaly);
    let has_hub = issues.iter().any(|issue| issue.kind == IssueType::Hub);
    let has_god_object = issues
        .iter()
        .any(|issue| issue.kind == IssueType::GodObject);

    if has_cycle && has_layer_violation && has_role_mismatch {
        return RootCause {
            label: "Layer Collapse".to_string(),
            confidence: 0.92,
        };
    }

    if has_layer_violation && has_role_mismatch {
        return RootCause {
            label: "Missing Application Layer".to_string(),
            confidence: 0.89,
        };
    }

    if has_hub || has_god_object {
        return RootCause {
            label: "Responsibility Collapse".to_string(),
            confidence: 0.78,
        };
    }

    if has_data_flow_anomaly {
        return RootCause {
            label: "Unbounded Data Flow".to_string(),
            confidence: 0.74,
        };
    }

    if has_cycle {
        return RootCause {
            label: "Circular Dependency".to_string(),
            confidence: 0.86,
        };
    }

    RootCause {
        label: "No Structural Root Cause".to_string(),
        confidence: 0.60,
    }
}

pub fn generate_plan(root: &RootCause) -> Vec<RefactorStep> {
    let steps = match root.label.as_str() {
        "Layer Collapse" => vec![
            "Introduce service layer",
            "Redirect renderer -> service",
            "Remove renderer -> world dependency",
        ],
        "Missing Application Layer" => vec![
            "Introduce service layer",
            "Move orchestration from interface modules into service",
            "Redirect presentation dependencies through service DTOs",
        ],
        "Responsibility Collapse" => vec![
            "Split high-coupling module by role",
            "Extract DTO boundary for presentation",
            "Re-run structural analysis after dependency reduction",
        ],
        "Unbounded Data Flow" => vec![
            "Introduce DTO mapping boundary",
            "Reduce direct cross-layer data access",
            "Constrain data flow through application services",
        ],
        "Circular Dependency" => vec![
            "Remove one edge from the cycle",
            "Introduce an intermediate abstraction if needed",
            "Re-validate layer directions",
        ],
        _ => vec!["No refactor required"],
    };

    steps
        .into_iter()
        .map(|description| RefactorStep {
            description: description.to_string(),
        })
        .collect()
}
