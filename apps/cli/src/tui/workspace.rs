use crate::tui::state::{SpecificationEditor, UiEvent};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvaluationWorkspace {
    pub domain: String,
    pub status: String,
    pub progress: u8,
    pub violations: usize,
    pub warnings: usize,
    pub active_task: Option<String>,
}

impl Default for EvaluationWorkspace {
    fn default() -> Self {
        Self {
            domain: "(none)".to_string(),
            status: "Recognition".to_string(),
            progress: 0,
            violations: 0,
            warnings: 0,
            active_task: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AnalysisResultWorkspace {
    pub analyze_projection: Option<crate::tui::state::AnalyzeProjection>,
    pub diagnosis: Vec<String>,
    pub repair_plan: Vec<String>,
    pub implementation_plan: Vec<String>,
    pub error_message: Option<String>,
    pub execution_trace: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceState {
    pub evaluation: EvaluationWorkspace,
    pub analysis_result: AnalysisResultWorkspace,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DashboardState {
    pub specification_editor: SpecificationEditor,
    pub evaluation: EvaluationWorkspace,
    pub analysis_result: AnalysisResultWorkspace,
}

pub struct WorkspaceProjector;

impl WorkspaceProjector {
    pub fn project(workspace: &mut WorkspaceState, event: &UiEvent) {
        match event {
            UiEvent::DomainClassification { domain } => {
                workspace.evaluation.domain = domain.clone();
                workspace.evaluation.status = "Recognition".to_string();
                workspace.evaluation.progress = 25;
                workspace.evaluation.active_task = Some("Domain Classification".to_string());
            }
            UiEvent::StructuralDiagnosis { result } => {
                workspace.analysis_result.diagnosis = result
                    .violations
                    .iter()
                    .map(|violation| violation.rule.clone())
                    .chain(result.warnings.iter().map(|warning| warning.rule.clone()))
                    .collect();
                workspace.evaluation.status = "Diagnosis".to_string();
                workspace.evaluation.progress = 50;
                workspace.evaluation.violations = result.violations.len();
                workspace.evaluation.warnings = result.warnings.len();
                workspace.evaluation.active_task = Some("Structural Diagnosis".to_string());
                crate::tui::render_trace::record("workspace_diagnosis_projected");
            }
            UiEvent::AnalyzeResult { projection } => {
                workspace.analysis_result.analyze_projection = Some(projection.clone());
                workspace.analysis_result.diagnosis = projection.findings.clone();
                workspace.analysis_result.repair_plan = projection.mutation_candidates.clone();
                workspace.evaluation.domain = projection.project_name.clone();
                workspace.evaluation.status = "Analyzed".to_string();
                workspace.evaluation.progress = 100;
                workspace.evaluation.violations = projection.dependency_cycles;
                workspace.evaluation.active_task = None;
                crate::tui::render_trace::record("workspace_analyze_result_projected");
            }
            UiEvent::RepairPlan { plan } => {
                workspace.analysis_result.repair_plan = plan
                    .suggestions
                    .iter()
                    .map(|suggestion| suggestion.title.clone())
                    .collect();
                workspace.evaluation.status = "RepairPlanning".to_string();
                workspace.evaluation.progress = 75;
                workspace.evaluation.active_task = plan
                    .suggestions
                    .first()
                    .map(|suggestion| suggestion.title.clone());
                crate::tui::render_trace::record("workspace_repair_plan_projected");
            }
            UiEvent::ImplementationPlan { plan } => {
                workspace.analysis_result.implementation_plan =
                    plan.tasks.iter().map(|task| task.title.clone()).collect();
                workspace.evaluation.status = "Completed".to_string();
                workspace.evaluation.progress = 100;
                workspace.evaluation.active_task =
                    plan.tasks.first().map(|task| task.title.clone());
                crate::tui::render_trace::record("workspace_implementation_plan_projected");
            }
            UiEvent::Thinking { summary } => {
                workspace.evaluation.status = "Thinking".to_string();
                workspace.evaluation.progress = workspace.evaluation.progress.max(10);
                workspace.evaluation.active_task = runtime_task_id(summary);
                workspace
                    .analysis_result
                    .execution_trace
                    .push(summary.clone());
            }
            UiEvent::Planning { summary } => {
                workspace.evaluation.status = "Planning".to_string();
                workspace.evaluation.progress = workspace.evaluation.progress.max(25);
                workspace.evaluation.active_task =
                    runtime_task_id(summary).or_else(|| workspace.evaluation.active_task.clone());
                workspace
                    .analysis_result
                    .execution_trace
                    .push(summary.clone());
            }
            UiEvent::Pipeline { state } => {
                workspace.evaluation.status = "Planning".to_string();
                workspace.evaluation.progress = workspace.evaluation.progress.max(35);
                workspace
                    .analysis_result
                    .execution_trace
                    .push(state.clone());
            }
            UiEvent::Execution { step } => {
                workspace.evaluation.status = "Executing".to_string();
                workspace.evaluation.progress = workspace.evaluation.progress.max(60);
                workspace.evaluation.active_task =
                    runtime_task_id(step).or_else(|| workspace.evaluation.active_task.clone());
                workspace.analysis_result.execution_trace.push(step.clone());
            }
            UiEvent::Runtime { message } => {
                workspace
                    .analysis_result
                    .execution_trace
                    .push(message.clone());
                if let Some(task_id) = runtime_task_id(message) {
                    workspace.evaluation.active_task = Some(task_id);
                }
            }
            UiEvent::Result { message } => {
                workspace.evaluation.status = "Completed".to_string();
                workspace.evaluation.progress = 100;
                record_active_task_clear(workspace.evaluation.active_task.as_deref());
                workspace.evaluation.active_task = None;
                workspace.analysis_result.error_message = None;
                workspace
                    .analysis_result
                    .execution_trace
                    .push(message.clone());
                project_runtime_result(&mut workspace.analysis_result, message);
            }
            UiEvent::System { summary } if is_terminal_runtime_summary(summary) => {
                workspace.evaluation.status = "Completed".to_string();
                workspace.evaluation.progress = 100;
                record_active_task_clear(workspace.evaluation.active_task.as_deref());
                workspace.evaluation.active_task = None;
                workspace
                    .analysis_result
                    .execution_trace
                    .push(summary.clone());
            }
            UiEvent::Error { message } => {
                workspace.evaluation.status = "Failed".to_string();
                record_active_task_clear(workspace.evaluation.active_task.as_deref());
                workspace.evaluation.active_task = None;
                workspace.analysis_result.error_message = Some(message.clone());
                workspace
                    .analysis_result
                    .execution_trace
                    .push(message.clone());
            }
            _ => {}
        }
    }
}

impl AnalysisResultWorkspace {
    pub fn lines(&self) -> Vec<String> {
        if let Some(projection) = &self.analyze_projection {
            return projection.render().lines().map(str::to_string).collect();
        }
        let mut lines = Vec::new();
        lines.push("Diagnosis".to_string());
        lines.extend(item_lines(&self.diagnosis));
        lines.push(String::new());
        lines.push("Repair Plan".to_string());
        lines.extend(item_lines(&self.repair_plan));
        lines.push(String::new());
        lines.push("Implementation Plan".to_string());
        lines.extend(item_lines(&self.implementation_plan));
        if let Some(message) = &self.error_message {
            lines.push(String::new());
            lines.push("Error Message".to_string());
            lines.push(format!("- {message}"));
        }
        if !self.execution_trace.is_empty() {
            lines.push(String::new());
            lines.push("Execution Trace".to_string());
            lines.extend(item_lines(&self.execution_trace));
        }
        lines
    }
}

impl EvaluationWorkspace {
    pub fn lines(&self) -> Vec<String> {
        vec![
            "Domain:".to_string(),
            self.domain.clone(),
            String::new(),
            "Status:".to_string(),
            self.status.clone(),
            String::new(),
            "Progress:".to_string(),
            format!("{}%", self.progress),
            String::new(),
            "Violations:".to_string(),
            self.violations.to_string(),
            String::new(),
            "Warnings:".to_string(),
            self.warnings.to_string(),
            String::new(),
            "Active Task:".to_string(),
            self.active_task
                .clone()
                .unwrap_or_else(|| "(none)".to_string()),
        ]
    }
}

fn item_lines(items: &[String]) -> Vec<String> {
    if items.is_empty() {
        vec!["- none".to_string()]
    } else {
        items.iter().map(|item| format!("- {item}")).collect()
    }
}

fn runtime_task_id(text: &str) -> Option<String> {
    let task = text.split_whitespace().collect::<Vec<_>>();
    task.windows(2).find_map(|window| {
        if window[0] == "task" {
            let id = window[1].trim_matches(|ch: char| !ch.is_ascii_alphanumeric());
            if !id.is_empty() {
                return Some(format!("task-{id}"));
            }
        }
        None
    })
}

fn is_terminal_runtime_summary(summary: &str) -> bool {
    let lower = summary.to_ascii_lowercase();
    lower.contains("completed") || lower.contains("cancelled")
}

fn record_active_task_clear(before: Option<&str>) {
    crate::tui::render_trace::record(Box::leak(
        format!(
            "[ACTIVE_TASK_CLEAR]\nbefore={}\nafter=None",
            before.unwrap_or("(none)")
        )
        .into_boxed_str(),
    ));
}

fn project_runtime_result(workspace: &mut AnalysisResultWorkspace, message: &str) {
    let sections = parse_runtime_result_sections(message);
    if let Some(diagnosis) = sections.diagnosis {
        workspace.diagnosis = diagnosis;
    } else if workspace.diagnosis.is_empty() {
        workspace.diagnosis = vec![message.to_string()];
    }
    if let Some(repair_plan) = sections.repair_plan {
        workspace.repair_plan = repair_plan;
    }
    if let Some(implementation_plan) = sections.implementation_plan {
        workspace.implementation_plan = implementation_plan;
    }
}

#[derive(Default)]
struct RuntimeResultSections {
    diagnosis: Option<Vec<String>>,
    repair_plan: Option<Vec<String>>,
    implementation_plan: Option<Vec<String>>,
}

fn parse_runtime_result_sections(message: &str) -> RuntimeResultSections {
    #[derive(Clone, Copy)]
    enum Section {
        Diagnosis,
        RepairPlan,
        ImplementationPlan,
    }

    let mut sections = RuntimeResultSections::default();
    let mut current: Option<Section> = None;

    for raw in message.lines() {
        let line = raw
            .trim()
            .trim_start_matches('#')
            .trim()
            .trim_end_matches(':')
            .trim();
        let normalized = line.to_ascii_lowercase().replace(['-', '_'], " ");
        current = match normalized.as_str() {
            "diagnosis" => Some(Section::Diagnosis),
            "repair plan" => Some(Section::RepairPlan),
            "implementation plan" => Some(Section::ImplementationPlan),
            _ => current,
        };
        if matches!(
            normalized.as_str(),
            "diagnosis" | "repair plan" | "implementation plan"
        ) {
            continue;
        }
        if line.is_empty() {
            continue;
        }
        let item = line.trim_start_matches(['-', '*']).trim().to_string();
        match current {
            Some(Section::Diagnosis) => sections.diagnosis.get_or_insert_with(Vec::new).push(item),
            Some(Section::RepairPlan) => {
                sections.repair_plan.get_or_insert_with(Vec::new).push(item)
            }
            Some(Section::ImplementationPlan) => sections
                .implementation_plan
                .get_or_insert_with(Vec::new)
                .push(item),
            None => {}
        }
    }

    sections
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

    #[test]
    fn runtime_events_project_status_active_task_and_result() {
        let mut workspace = WorkspaceState::default();

        WorkspaceProjector::project(
            &mut workspace,
            &UiEvent::Thinking {
                summary: "task 42 queued".to_string(),
            },
        );
        assert_eq!(workspace.evaluation.status, "Thinking");
        assert_eq!(workspace.evaluation.active_task.as_deref(), Some("task-42"));

        WorkspaceProjector::project(
            &mut workspace,
            &UiEvent::Planning {
                summary: "task 42 planning runtime execution".to_string(),
            },
        );
        assert_eq!(workspace.evaluation.status, "Planning");
        assert_eq!(workspace.evaluation.active_task.as_deref(), Some("task-42"));

        WorkspaceProjector::project(
            &mut workspace,
            &UiEvent::Execution {
                step: "task 42 executing runtime core".to_string(),
            },
        );
        assert_eq!(workspace.evaluation.status, "Executing");
        assert_eq!(workspace.evaluation.active_task.as_deref(), Some("task-42"));

        WorkspaceProjector::project(
            &mut workspace,
            &UiEvent::Result {
                message: "Diagnosis\n- RuntimeProjectionMissing\n\nRepair Plan\n- Project runtime events\n\nImplementation Plan\n- Render Runtime Activity".to_string(),
            },
        );
        assert_eq!(workspace.evaluation.status, "Completed");
        assert_eq!(workspace.evaluation.active_task, None);
        assert_eq!(
            workspace.analysis_result.diagnosis,
            vec!["RuntimeProjectionMissing".to_string()]
        );
        assert_eq!(
            workspace.analysis_result.repair_plan,
            vec!["Project runtime events".to_string()]
        );
        assert_eq!(
            workspace.analysis_result.implementation_plan,
            vec!["Render Runtime Activity".to_string()]
        );
    }

    #[test]
    fn runtime_error_projects_failed_and_clears_active_task() {
        let mut workspace = WorkspaceState::default();
        WorkspaceProjector::project(
            &mut workspace,
            &UiEvent::Thinking {
                summary: "task 7 queued".to_string(),
            },
        );

        WorkspaceProjector::project(
            &mut workspace,
            &UiEvent::Error {
                message: "task 7 failed".to_string(),
            },
        );

        assert_eq!(workspace.evaluation.status, "Failed");
        assert_eq!(workspace.evaluation.active_task, None);
        assert_eq!(
            workspace.analysis_result.error_message.as_deref(),
            Some("task 7 failed")
        );
    }
}
