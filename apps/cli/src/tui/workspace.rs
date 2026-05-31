use crate::specification_bridge::{RepairPriority, SpecificationContext};
use crate::tui::state::UiEvent;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SpecificationWorkspace {
    pub system_name: Option<String>,
    pub goals: Vec<String>,
    pub constraints: Vec<String>,
    pub architecture_summary: Vec<String>,
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage {
    Recognition,
    Diagnosis,
    RepairPlan,
    ImplementationPlan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStatus {
    Idle,
    Running,
    Completed,
    Failed,
}

impl Default for PipelineStatus {
    fn default() -> Self {
        Self::Idle
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PipelineWorkspace {
    pub recognition: PipelineStatus,
    pub diagnosis: PipelineStatus,
    pub repair_plan: PipelineStatus,
    pub implementation_plan: PipelineStatus,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TaskWorkspace {
    pub repair_suggestions: Vec<String>,
    pub implementation_tasks: Vec<String>,
    pub validation_tasks: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceState {
    pub specification: SpecificationWorkspace,
    pub pipeline: PipelineWorkspace,
    pub tasks: TaskWorkspace,
}

pub struct WorkspaceProjector;

impl WorkspaceProjector {
    pub fn project(workspace: &mut WorkspaceState, event: &UiEvent) {
        match event {
            UiEvent::SpecContext { context } => {
                project_spec_context(workspace, context);
            }
            UiEvent::StructuralDiagnosis { .. } => {
                workspace.pipeline.recognition = PipelineStatus::Completed;
                workspace.pipeline.diagnosis = PipelineStatus::Completed;
            }
            UiEvent::RepairPlan { plan } => {
                workspace.pipeline.repair_plan = PipelineStatus::Completed;
                workspace.tasks.repair_suggestions = plan
                    .suggestions
                    .iter()
                    .map(|suggestion| {
                        format!(
                            "[{}] {}",
                            repair_priority_label(suggestion.priority),
                            suggestion.title
                        )
                    })
                    .collect();
            }
            UiEvent::ImplementationPlan { plan } => {
                workspace.pipeline.implementation_plan = PipelineStatus::Completed;
                workspace.tasks.implementation_tasks =
                    plan.tasks.iter().map(|task| task.title.clone()).collect();
                workspace.tasks.validation_tasks = plan
                    .validations
                    .iter()
                    .map(|validation| validation.description.clone())
                    .collect();
            }
            UiEvent::Runtime { message }
            | UiEvent::System { summary: message }
            | UiEvent::Result { message }
            | UiEvent::Pipeline { state: message }
            | UiEvent::Debug { message } => project_text_event(workspace, message),
            _ => {}
        }
    }
}

impl SpecificationWorkspace {
    pub fn lines(&self) -> Vec<String> {
        let mut lines = vec![
            format!(
                "system_name={}",
                self.system_name.as_deref().unwrap_or("(none)")
            ),
            format!("goals={}", self.goals.len()),
            format!("constraints={}", self.constraints.len()),
            format!("architecture={}", self.architecture_summary.len()),
            format!("rules={}", self.rules.len()),
        ];
        lines.extend(section_lines("Goals", &self.goals));
        lines.extend(section_lines("Constraints", &self.constraints));
        lines.extend(section_lines("Architecture", &self.architecture_summary));
        lines.extend(section_lines("Rules", &self.rules));
        lines
    }
}

impl PipelineWorkspace {
    pub fn lines(&self) -> Vec<String> {
        vec![
            stage_line(PipelineStage::Recognition, self.recognition),
            stage_line(PipelineStage::Diagnosis, self.diagnosis),
            stage_line(PipelineStage::RepairPlan, self.repair_plan),
            stage_line(PipelineStage::ImplementationPlan, self.implementation_plan),
        ]
    }
}

impl TaskWorkspace {
    pub fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push("Repair Suggestions".to_string());
        lines.extend(item_lines(&self.repair_suggestions));
        lines.push(String::new());
        lines.push("Implementation Tasks".to_string());
        lines.extend(item_lines(&self.implementation_tasks));
        lines.push(String::new());
        lines.push("Validation Tasks".to_string());
        lines.extend(item_lines(&self.validation_tasks));
        lines
    }
}

fn project_spec_context(workspace: &mut WorkspaceState, context: &SpecificationContext) {
    workspace.specification = SpecificationWorkspace {
        system_name: context.system_name.clone(),
        goals: context.goals.clone(),
        constraints: context.constraints.clone(),
        architecture_summary: context
            .architecture
            .iter()
            .map(|component| {
                if component.responsibilities.is_empty() {
                    component.name.clone()
                } else {
                    format!(
                        "{}: {}",
                        component.name,
                        component.responsibilities.join(", ")
                    )
                }
            })
            .collect(),
        rules: context.rules.clone(),
    };
    workspace.pipeline.recognition = PipelineStatus::Completed;
}

fn project_text_event(workspace: &mut WorkspaceState, message: &str) {
    if message.contains("[SPEC_CONTEXT]") {
        workspace.pipeline.recognition = PipelineStatus::Completed;
    }
    if message.contains("[STRUCTURAL_DIAGNOSIS]") {
        workspace.pipeline.diagnosis = if message.contains("status=started") {
            PipelineStatus::Running
        } else {
            PipelineStatus::Completed
        };
    }
    if message.contains("[REPAIR_PLAN]") {
        workspace.pipeline.repair_plan = PipelineStatus::Completed;
    }
    if message.contains("[IMPLEMENTATION_PLAN]") || message.contains("[IMPLEMENTATION_PLANNING]") {
        workspace.pipeline.implementation_plan = if message.contains("status=started") {
            PipelineStatus::Running
        } else {
            PipelineStatus::Completed
        };
    }
}

fn section_lines(title: &str, items: &[String]) -> Vec<String> {
    if items.is_empty() {
        return Vec::new();
    }
    let mut lines = vec![String::new(), title.to_string()];
    lines.extend(item_lines(items));
    lines
}

fn item_lines(items: &[String]) -> Vec<String> {
    if items.is_empty() {
        vec!["- none".to_string()]
    } else {
        items.iter().map(|item| format!("- {item}")).collect()
    }
}

fn stage_line(stage: PipelineStage, status: PipelineStatus) -> String {
    format!(
        "{:<22} {}",
        stage_label(stage),
        pipeline_status_label(status)
    )
}

fn stage_label(stage: PipelineStage) -> &'static str {
    match stage {
        PipelineStage::Recognition => "Recognition",
        PipelineStage::Diagnosis => "Diagnosis",
        PipelineStage::RepairPlan => "RepairPlan",
        PipelineStage::ImplementationPlan => "ImplementationPlan",
    }
}

fn pipeline_status_label(status: PipelineStatus) -> &'static str {
    match status {
        PipelineStatus::Idle => "- Idle",
        PipelineStatus::Running => "... Running",
        PipelineStatus::Completed => "✔ Completed",
        PipelineStatus::Failed => "! Failed",
    }
}

fn repair_priority_label(priority: RepairPriority) -> &'static str {
    match priority {
        RepairPriority::Critical => "Critical",
        RepairPriority::Recommended => "Recommended",
        RepairPriority::Optional => "Optional",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::specification_bridge::{
        ImplementationPlan, ImplementationTask, RepairImpact, RepairPlan, RepairPlanner,
        RepairSuggestion, StructuralDiagnosisRequest, StructuralDiagnosisResult, ValidationPlan,
    };

    #[test]
    fn spec_context_projects_to_specification_workspace() {
        let context = SpecificationContext::from_yaml(
            "system_name: DBM_REPL_UI\nrules:\n  - Runtime must pass through AuditCore\n",
        )
        .expect("context");
        let mut workspace = WorkspaceState::default();

        WorkspaceProjector::project(&mut workspace, &UiEvent::SpecContext { context });

        assert_eq!(
            workspace.specification.system_name.as_deref(),
            Some("DBM_REPL_UI")
        );
        assert_eq!(workspace.specification.rules.len(), 1);
        assert_eq!(workspace.pipeline.recognition, PipelineStatus::Completed);
    }

    #[test]
    fn structural_diagnosis_projects_pipeline_status() {
        let mut workspace = WorkspaceState::default();

        WorkspaceProjector::project(
            &mut workspace,
            &UiEvent::StructuralDiagnosis {
                result: StructuralDiagnosisResult {
                    violations: Vec::new(),
                    warnings: Vec::new(),
                },
            },
        );

        assert_eq!(workspace.pipeline.diagnosis, PipelineStatus::Completed);
    }

    #[test]
    fn repair_plan_projects_task_workspace() {
        let mut workspace = WorkspaceState::default();

        WorkspaceProjector::project(
            &mut workspace,
            &UiEvent::RepairPlan {
                plan: RepairPlan {
                    suggestions: vec![RepairSuggestion {
                        id: "applygate-missing".to_string(),
                        title: "Enforce ApplyGate".to_string(),
                        rationale: "Direct mutation path detected".to_string(),
                        impact: RepairImpact::High,
                        priority: RepairPriority::Critical,
                    }],
                    execution_steps: Vec::new(),
                },
            },
        );

        assert_eq!(workspace.pipeline.repair_plan, PipelineStatus::Completed);
        assert!(
            workspace
                .tasks
                .repair_suggestions
                .contains(&"[Critical] Enforce ApplyGate".to_string())
        );
    }

    #[test]
    fn implementation_plan_projects_tasks_and_validations() {
        let mut workspace = WorkspaceState::default();

        WorkspaceProjector::project(
            &mut workspace,
            &UiEvent::ImplementationPlan {
                plan: ImplementationPlan {
                    tasks: vec![ImplementationTask {
                        id: "impl-applygate".to_string(),
                        title: "Route mutation through ApplyGate".to_string(),
                        description: String::new(),
                        target_component: "runtime layer".to_string(),
                        priority: crate::specification_bridge::ImplementationPriority::Critical,
                    }],
                    file_modifications: Vec::new(),
                    validations: vec![ValidationPlan {
                        validation_type: "IntegrationTest".to_string(),
                        description: "ApplyGate integration test".to_string(),
                    }],
                },
            },
        );

        assert_eq!(
            workspace.pipeline.implementation_plan,
            PipelineStatus::Completed
        );
        assert!(
            workspace
                .tasks
                .implementation_tasks
                .contains(&"Route mutation through ApplyGate".to_string())
        );
        assert!(
            workspace
                .tasks
                .validation_tasks
                .contains(&"ApplyGate integration test".to_string())
        );
    }

    #[test]
    fn design_specification_pipeline_e2e_projects_all_workspaces() {
        let context = SpecificationContext::from_yaml(
            "system_name: DBM_REPL_UI\nrules:\n  - Runtime must pass through AuditCore\n",
        )
        .expect("context");
        let diagnosis = StructuralDiagnosisRequest::new(context.clone()).diagnose();
        let repair_plan = RepairPlanner::generate(&diagnosis);
        let implementation_plan =
            crate::specification_bridge::ImplementationPlanner::generate(&repair_plan);
        let mut workspace = WorkspaceState::default();

        WorkspaceProjector::project(&mut workspace, &UiEvent::SpecContext { context });
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

        assert_eq!(
            workspace.specification.system_name.as_deref(),
            Some("DBM_REPL_UI")
        );
        assert_eq!(workspace.specification.rules.len(), 1);
        assert_eq!(workspace.pipeline.recognition, PipelineStatus::Completed);
        assert_eq!(workspace.pipeline.diagnosis, PipelineStatus::Completed);
        assert_eq!(workspace.pipeline.repair_plan, PipelineStatus::Completed);
        assert_eq!(
            workspace.pipeline.implementation_plan,
            PipelineStatus::Completed
        );
        assert!(
            workspace
                .tasks
                .repair_suggestions
                .iter()
                .any(|line| line.contains("Enforce ApplyGate"))
        );
        assert!(
            workspace
                .tasks
                .implementation_tasks
                .iter()
                .any(|line| line == "Route mutation through ApplyGate")
        );
        assert!(
            workspace
                .tasks
                .validation_tasks
                .iter()
                .any(|line| line == "ApplyGate integration test")
        );
    }
}
