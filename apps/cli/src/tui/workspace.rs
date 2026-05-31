use crate::tui::state::UiEvent;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnalysisResultWorkspace {
    pub diagnosis: Vec<String>,
    pub repair_plan: Vec<String>,
    pub implementation_plan: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceState {
    pub analysis_result: AnalysisResultWorkspace,
}

pub struct WorkspaceProjector;

impl WorkspaceProjector {
    pub fn project(workspace: &mut WorkspaceState, event: &UiEvent) {
        match event {
            UiEvent::StructuralDiagnosis { result } => {
                workspace.analysis_result.diagnosis = result
                    .violations
                    .iter()
                    .map(|violation| violation.rule.clone())
                    .chain(result.warnings.iter().map(|warning| warning.rule.clone()))
                    .collect();
            }
            UiEvent::RepairPlan { plan } => {
                workspace.analysis_result.repair_plan = plan
                    .suggestions
                    .iter()
                    .map(|suggestion| suggestion.title.clone())
                    .collect();
            }
            UiEvent::ImplementationPlan { plan } => {
                workspace.analysis_result.implementation_plan =
                    plan.tasks.iter().map(|task| task.title.clone()).collect();
            }
            _ => {}
        }
    }
}

impl AnalysisResultWorkspace {
    pub fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push("Diagnosis".to_string());
        lines.extend(item_lines(&self.diagnosis));
        lines.push(String::new());
        lines.push("Repair Plan".to_string());
        lines.extend(item_lines(&self.repair_plan));
        lines.push(String::new());
        lines.push("Implementation Plan".to_string());
        lines.extend(item_lines(&self.implementation_plan));
        lines
    }
}

fn item_lines(items: &[String]) -> Vec<String> {
    if items.is_empty() {
        vec!["- none".to_string()]
    } else {
        items.iter().map(|item| format!("- {item}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::specification_bridge::{
        ImplementationPlan, ImplementationPlanner, ImplementationPriority, ImplementationTask,
        RepairImpact, RepairPlan, RepairPlanner, RepairPriority, RepairSuggestion,
        SpecificationContext, StructuralDiagnosisRequest, StructuralDiagnosisResult,
        ValidationPlan, Violation, Warning,
    };

    #[test]
    fn structural_diagnosis_projects_to_analysis_result() {
        let mut workspace = WorkspaceState::default();

        WorkspaceProjector::project(
            &mut workspace,
            &UiEvent::StructuralDiagnosis {
                result: StructuralDiagnosisResult {
                    violations: vec![Violation {
                        rule: "WorkspaceVisibilityViolation".to_string(),
                        message: "missing workspace".to_string(),
                    }],
                    warnings: vec![Warning {
                        rule: "TimelineVisibilityViolation".to_string(),
                        message: "timeline hidden".to_string(),
                    }],
                },
            },
        );

        assert_eq!(
            workspace.analysis_result.diagnosis,
            vec![
                "WorkspaceVisibilityViolation".to_string(),
                "TimelineVisibilityViolation".to_string()
            ]
        );
    }

    #[test]
    fn repair_plan_projects_to_analysis_result() {
        let mut workspace = WorkspaceState::default();

        WorkspaceProjector::project(
            &mut workspace,
            &UiEvent::RepairPlan {
                plan: RepairPlan {
                    suggestions: vec![RepairSuggestion {
                        id: "timeline-missing".to_string(),
                        title: "Create Event Timeline".to_string(),
                        rationale: "Events are not visible".to_string(),
                        impact: RepairImpact::High,
                        priority: RepairPriority::Critical,
                    }],
                    execution_steps: Vec::new(),
                },
            },
        );

        assert_eq!(
            workspace.analysis_result.repair_plan,
            vec!["Create Event Timeline".to_string()]
        );
    }

    #[test]
    fn implementation_plan_projects_to_analysis_result() {
        let mut workspace = WorkspaceState::default();

        WorkspaceProjector::project(
            &mut workspace,
            &UiEvent::ImplementationPlan {
                plan: ImplementationPlan {
                    tasks: vec![ImplementationTask {
                        id: "impl-timeline".to_string(),
                        title: "Add TimelineRenderer".to_string(),
                        description: String::new(),
                        target_component: "EventTimeline".to_string(),
                        priority: ImplementationPriority::Critical,
                    }],
                    file_modifications: Vec::new(),
                    validations: vec![ValidationPlan {
                        validation_type: "UIStructuralReDiagnosis".to_string(),
                        description: "Timeline visibility re-run".to_string(),
                    }],
                },
            },
        );

        assert_eq!(
            workspace.analysis_result.implementation_plan,
            vec!["Add TimelineRenderer".to_string()]
        );
    }

    #[test]
    fn design_specification_pipeline_e2e_projects_analysis_result() {
        let context = SpecificationContext::from_yaml(
            "system_name: DBM_REPL_UI\narchitecture:\n  DesignWorkspace:\n",
        )
        .expect("context");
        let diagnosis = StructuralDiagnosisRequest::new(context).diagnose();
        let repair_plan = RepairPlanner::generate(&diagnosis);
        let implementation_plan = ImplementationPlanner::generate(&repair_plan);
        let mut workspace = WorkspaceState::default();

        WorkspaceProjector::project(
            &mut workspace,
            &UiEvent::StructuralDiagnosis { result: diagnosis },
        );
        WorkspaceProjector::project(&mut workspace, &UiEvent::RepairPlan { plan: repair_plan });
        WorkspaceProjector::project(
            &mut workspace,
            &UiEvent::ImplementationPlan {
                plan: implementation_plan,
            },
        );

        assert!(
            workspace
                .analysis_result
                .lines()
                .contains(&"Diagnosis".to_string())
        );
        assert!(!workspace.analysis_result.repair_plan.is_empty());
        assert!(!workspace.analysis_result.implementation_plan.is_empty());
    }
}
