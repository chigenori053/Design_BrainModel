use serde_yaml::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecificationKind {
    Instruction,
    DesignSpecification,
}

pub struct DesignSpecificationRecognizer;

impl DesignSpecificationRecognizer {
    pub fn is_design_specification_start(line: &str) -> bool {
        design_specification_key(line).is_some()
    }

    pub fn recognition_reason(line: &str) -> Option<&'static str> {
        design_specification_key(line)
    }
}

pub fn is_design_specification_start(line: &str) -> bool {
    DesignSpecificationRecognizer::is_design_specification_start(line)
}

fn design_specification_key(line: &str) -> Option<&'static str> {
    let line = line.trim_end();
    [
        "system_name",
        "goals",
        "constraints",
        "architecture",
        "rules",
        "components",
        "interfaces",
        "dependencies",
        "layers",
        "security",
        "memory",
        "runtime",
        "audit",
    ]
    .into_iter()
    .find(|key| {
        let prefix = format!("{key}:");
        line.starts_with(&prefix)
    })
}

pub fn classify_specification(text: &str) -> SpecificationKind {
    let has_design_key = text
        .lines()
        .any(|line| DesignSpecificationRecognizer::is_design_specification_start(line));
    if has_design_key {
        return SpecificationKind::DesignSpecification;
    }

    SpecificationKind::Instruction
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosisDomain {
    CoreArchitecture,
    RuntimeSafety,
    UserInterface,
}

impl DiagnosisDomain {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CoreArchitecture => "CoreArchitecture",
            Self::RuntimeSafety => "RuntimeSafety",
            Self::UserInterface => "UserInterface",
        }
    }

    pub fn diagnosis_log_label(self) -> &'static str {
        match self {
            Self::CoreArchitecture => "CORE_DIAGNOSIS",
            Self::RuntimeSafety => "RUNTIME_DIAGNOSIS",
            Self::UserInterface => "UI_DIAGNOSIS",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainReason {
    DefaultCoreArchitecture,
    RuntimeSafetyKeywordDetected,
    UiKeywordThresholdExceeded,
}

impl DomainReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DefaultCoreArchitecture => "default_core_architecture",
            Self::RuntimeSafetyKeywordDetected => "runtime_safety_keyword_detected",
            Self::UiKeywordThresholdExceeded => "ui_keyword_threshold_exceeded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpecificationContext {
    pub system_name: Option<String>,
    pub goals: Vec<String>,
    pub constraints: Vec<String>,
    pub architecture: Vec<ComponentSpec>,
    pub rules: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentSpec {
    pub name: String,
    pub responsibilities: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecificationError {
    InvalidYaml(String),
    InvalidStructure(String),
}

impl std::fmt::Display for SpecificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidYaml(message) => write!(f, "invalid yaml: {message}"),
            Self::InvalidStructure(message) => write!(f, "invalid structure: {message}"),
        }
    }
}

impl std::error::Error for SpecificationError {}

impl SpecificationContext {
    pub fn from_yaml(text: &str) -> Result<Self, SpecificationError> {
        let root = serde_yaml::from_str::<Value>(text)
            .map_err(|err| SpecificationError::InvalidYaml(err.to_string()))?;
        let map = root
            .as_mapping()
            .ok_or_else(|| SpecificationError::InvalidStructure("root must be a mapping".into()))?;

        Ok(Self {
            system_name: optional_string(
                map.get(Value::String("system_name".into())),
                "system_name",
            )?,
            goals: string_list(map.get(Value::String("goals".into())), "goals")?,
            constraints: string_list(map.get(Value::String("constraints".into())), "constraints")?,
            architecture: component_list(map.get(Value::String("architecture".into())))?,
            rules: string_list(map.get(Value::String("rules".into())), "rules")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralDiagnosisRequest {
    pub specification: SpecificationContext,
    pub domain: DiagnosisDomain,
    pub domain_reason: DomainReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructuralDiagnosisResult {
    pub violations: Vec<Violation>,
    pub warnings: Vec<Warning>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Warning {
    pub rule: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairImpact {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepairPriority {
    Critical,
    Recommended,
    Optional,
}

impl std::fmt::Display for RepairPriority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Critical => write!(f, "Critical"),
            Self::Recommended => write!(f, "Recommended"),
            Self::Optional => write!(f, "Optional"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairSuggestion {
    pub id: String,
    pub title: String,
    pub rationale: String,
    pub impact: RepairImpact,
    pub priority: RepairPriority,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairStep {
    pub order: usize,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairPlan {
    pub suggestions: Vec<RepairSuggestion>,
    pub execution_steps: Vec<RepairStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImplementationPriority {
    Critical,
    High,
    Normal,
    Low,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplementationTask {
    pub id: String,
    pub title: String,
    pub description: String,
    pub target_component: String,
    pub priority: ImplementationPriority,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileModificationPlan {
    pub target_file: String,
    pub action: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationPlan {
    pub validation_type: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImplementationPlan {
    pub tasks: Vec<ImplementationTask>,
    pub file_modifications: Vec<FileModificationPlan>,
    pub validations: Vec<ValidationPlan>,
}

pub struct RepairPlanner;
pub struct ImplementationPlanner;
pub struct UiStructuralDiagnosisEngine;

impl StructuralDiagnosisRequest {
    pub fn new(specification: SpecificationContext) -> Self {
        let selection = infer_diagnosis_domain(&specification);
        Self {
            specification,
            domain: selection.domain,
            domain_reason: selection.reason,
        }
    }

    pub fn with_domain(specification: SpecificationContext, domain: DiagnosisDomain) -> Self {
        Self {
            specification,
            domain,
            domain_reason: default_reason_for_domain(domain),
        }
    }

    pub fn diagnose(&self) -> StructuralDiagnosisResult {
        crate::tui::render_trace::record("specification_domain_classified");

        let diagnosis_label = self.domain.diagnosis_log_label();
        let _ = diagnosis_label;
        crate::tui::render_trace::record("specification_diagnosis_started");

        let result = match self.domain {
            DiagnosisDomain::UserInterface => {
                UiStructuralDiagnosisEngine::diagnose(&self.specification)
            }
            DiagnosisDomain::CoreArchitecture | DiagnosisDomain::RuntimeSafety => {
                run_structural_diagnosis(&self.specification)
            }
        };

        crate::tui::render_trace::record("specification_diagnosis_completed");

        result
    }
}

impl UiStructuralDiagnosisEngine {
    pub fn diagnose(context: &SpecificationContext) -> StructuralDiagnosisResult {
        let mut violations = Vec::new();
        let mut warnings = Vec::new();
        let architecture = UiArchitectureIndex::new(context);

        let required_workspaces = [
            "DesignWorkspace",
            "ActiveTaskWorkspace",
            "PipelineWorkspace",
        ];
        let missing_workspaces = required_workspaces
            .iter()
            .filter(|workspace| !architecture.has_component(workspace))
            .copied()
            .collect::<Vec<_>>();
        if !missing_workspaces.is_empty() {
            violations.push(Violation {
                rule: "WorkspaceVisibilityViolation".into(),
                message: format!(
                    "missing visible workspace(s): {}",
                    missing_workspaces.join(", ")
                ),
            });
        }

        let pipeline_stages = [
            "Recognition",
            "Diagnosis",
            "RepairPlan",
            "ImplementationPlan",
        ];
        let missing_pipeline_stages = pipeline_stages
            .iter()
            .filter(|stage| !architecture.has_responsibility(stage))
            .copied()
            .collect::<Vec<_>>();
        if !missing_pipeline_stages.is_empty() {
            violations.push(Violation {
                rule: "PipelineVisibilityViolation".into(),
                message: format!(
                    "missing pipeline visualization responsibility: {}",
                    missing_pipeline_stages.join(", ")
                ),
            });
        }

        let task_visibility = [
            "Repair Suggestions",
            "Implementation Tasks",
            "Validation Tasks",
        ];
        let missing_task_visibility = task_visibility
            .iter()
            .filter(|item| !architecture.has_responsibility(item))
            .copied()
            .collect::<Vec<_>>();
        if !missing_task_visibility.is_empty() {
            violations.push(Violation {
                rule: "TaskVisibilityViolation".into(),
                message: format!(
                    "missing task visualization responsibility: {}",
                    missing_task_visibility.join(", ")
                ),
            });
        }

        let timeline_events = ["Runtime Events", "Diagnosis Events", "Planning Events"];
        let missing_timeline_events = timeline_events
            .iter()
            .filter(|event| !architecture.has_responsibility(event))
            .copied()
            .collect::<Vec<_>>();
        if !missing_timeline_events.is_empty() {
            violations.push(Violation {
                rule: "TimelineVisibilityViolation".into(),
                message: format!(
                    "missing timeline responsibility: {}",
                    missing_timeline_events.join(", ")
                ),
            });
        }

        if !architecture.has_component("SpecificationEditor") {
            violations.push(Violation {
                rule: "InputAccessibilityViolation".into(),
                message: "SpecificationEditor is not defined".into(),
            });
        }

        if architecture.workspace_count > 8 {
            warnings.push(Warning {
                rule: "ExcessiveCognitiveLoad".into(),
                message: format!(
                    "workspace_count={} exceeds the recommended limit of 8",
                    architecture.workspace_count
                ),
            });
        }

        if let Some(duplicate) = architecture.duplicate_responsibility() {
            violations.push(Violation {
                rule: "InformationDuplicationViolation".into(),
                message: format!(
                    "responsibility '{}' is duplicated across workspaces",
                    duplicate
                ),
            });
        }

        if architecture.has_excessive_workspace_ratio() {
            violations.push(Violation {
                rule: "LayoutBalanceViolation".into(),
                message: "workspace occupancy exceeds 70%".into(),
            });
        }

        StructuralDiagnosisResult {
            violations,
            warnings,
        }
    }
}

pub fn run_structural_diagnosis(specification: &SpecificationContext) -> StructuralDiagnosisResult {
    let mut violations = Vec::new();
    let mut warnings = Vec::new();

    let all_architecture_text = specification
        .architecture
        .iter()
        .flat_map(|component| {
            std::iter::once(component.name.as_str())
                .chain(component.responsibilities.iter().map(String::as_str))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();

    let rules_text = specification
        .rules
        .iter()
        .map(|rule| rule.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("\n");

    if !rules_text.contains("applygate") {
        violations.push(Violation {
            rule: "ApplyGate required".into(),
            message: "rules must declare ApplyGate".into(),
        });
    } else if specification.architecture.is_empty() {
        violations.push(Violation {
            rule: "ApplyGate boundary unspecified".into(),
            message: "rules declare ApplyGate but architecture does not expose its boundary".into(),
        });
    }
    if rules_text.contains("cyclic dependencies allowed") {
        violations.push(Violation {
            rule: "No cyclic dependencies".into(),
            message: "rules allow cyclic dependencies".into(),
        });
    }
    if rules_text.contains("layer violation allowed")
        || rules_text.contains("layer violations allowed")
    {
        violations.push(Violation {
            rule: "No layer violations".into(),
            message: "rules allow layer violations".into(),
        });
    }
    if all_architecture_text.contains("runtime bypass auditcore") {
        violations.push(Violation {
            rule: "Runtime bypass AuditCore".into(),
            message: "architecture describes Runtime bypassing AuditCore".into(),
        });
    }
    if all_architecture_text.contains("memory write violation")
        || all_architecture_text.contains("write memory without applygate")
    {
        violations.push(Violation {
            rule: "Memory write violations".into(),
            message: "architecture describes an unguarded memory write".into(),
        });
    }
    if all_architecture_text.contains("layer crossing") {
        warnings.push(Warning {
            rule: "Layer crossing".into(),
            message: "architecture mentions layer crossing".into(),
        });
    }

    StructuralDiagnosisResult {
        violations,
        warnings,
    }
}

struct UiArchitectureIndex<'a> {
    components: &'a [ComponentSpec],
    normalized_text: String,
    workspace_count: usize,
}

impl<'a> UiArchitectureIndex<'a> {
    fn new(context: &'a SpecificationContext) -> Self {
        let normalized_text = specification_search_text(context);
        let workspace_count = context
            .architecture
            .iter()
            .filter(|component| component.name.to_ascii_lowercase().contains("workspace"))
            .count();

        Self {
            components: &context.architecture,
            normalized_text,
            workspace_count,
        }
    }

    fn has_component(&self, name: &str) -> bool {
        let expected = normalize_ui_token(name);
        self.components
            .iter()
            .any(|component| normalize_ui_token(&component.name) == expected)
    }

    fn has_responsibility(&self, responsibility: &str) -> bool {
        let expected = normalize_ui_token(responsibility);
        self.components.iter().any(|component| {
            component
                .responsibilities
                .iter()
                .any(|candidate| normalize_ui_token(candidate).contains(&expected))
        })
    }

    fn duplicate_responsibility(&self) -> Option<String> {
        let mut seen = Vec::<String>::new();
        for component in self
            .components
            .iter()
            .filter(|component| component.name.to_ascii_lowercase().contains("workspace"))
        {
            for responsibility in &component.responsibilities {
                let normalized = normalize_ui_token(responsibility);
                if normalized.is_empty() {
                    continue;
                }
                if seen.iter().any(|existing| existing == &normalized) {
                    return Some(responsibility.clone());
                }
                seen.push(normalized);
            }
        }
        None
    }

    fn has_excessive_workspace_ratio(&self) -> bool {
        self.normalized_text.contains("workspaceoccupancy>70")
            || self.normalized_text.contains("workspaceratio>70")
            || extract_percentages(&self.normalized_text)
                .into_iter()
                .any(|percentage| percentage > 70)
                && (self.normalized_text.contains("occupancy")
                    || self.normalized_text.contains("ratio"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DomainSelection {
    domain: DiagnosisDomain,
    reason: DomainReason,
}

fn infer_diagnosis_domain(context: &SpecificationContext) -> DomainSelection {
    const UI_KEYWORDS: [&str; 6] = [
        "Workspace",
        "Dashboard",
        "Timeline",
        "Editor",
        "Visualization",
        "Layout",
    ];

    let text = specification_search_text(context);
    let hits = UI_KEYWORDS
        .iter()
        .map(|keyword| text.matches(&normalize_ui_token(keyword)).count())
        .sum::<usize>();

    if hits >= 2 {
        DomainSelection {
            domain: DiagnosisDomain::UserInterface,
            reason: DomainReason::UiKeywordThresholdExceeded,
        }
    } else if text.contains("runtime") || text.contains("applygate") || text.contains("audit") {
        DomainSelection {
            domain: DiagnosisDomain::RuntimeSafety,
            reason: DomainReason::RuntimeSafetyKeywordDetected,
        }
    } else {
        DomainSelection {
            domain: DiagnosisDomain::CoreArchitecture,
            reason: DomainReason::DefaultCoreArchitecture,
        }
    }
}

fn default_reason_for_domain(domain: DiagnosisDomain) -> DomainReason {
    match domain {
        DiagnosisDomain::CoreArchitecture => DomainReason::DefaultCoreArchitecture,
        DiagnosisDomain::RuntimeSafety => DomainReason::RuntimeSafetyKeywordDetected,
        DiagnosisDomain::UserInterface => DomainReason::UiKeywordThresholdExceeded,
    }
}

fn specification_search_text(context: &SpecificationContext) -> String {
    let mut values = Vec::new();
    if let Some(system_name) = &context.system_name {
        values.push(system_name.as_str());
    }
    values.extend(context.goals.iter().map(String::as_str));
    values.extend(context.constraints.iter().map(String::as_str));
    values.extend(context.rules.iter().map(String::as_str));
    for component in &context.architecture {
        values.push(component.name.as_str());
        values.extend(component.responsibilities.iter().map(String::as_str));
    }
    normalize_ui_token(&values.join("\n"))
}

fn normalize_ui_token(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '>' || *ch == '%')
        .flat_map(char::to_lowercase)
        .collect()
}

fn extract_percentages(text: &str) -> Vec<u32> {
    let mut percentages = Vec::new();
    let bytes = text.as_bytes();
    for index in 0..bytes.len() {
        if bytes[index] != b'%' {
            continue;
        }
        let mut start = index;
        while start > 0 && bytes[start - 1].is_ascii_digit() {
            start -= 1;
        }
        if start < index
            && let Ok(value) = text[start..index].parse::<u32>()
        {
            percentages.push(value);
        }
    }
    percentages
}

impl RepairPlanner {
    pub fn generate(diagnosis: &StructuralDiagnosisResult) -> RepairPlan {
        let mut suggestions = Vec::new();
        let mut step_descriptions = Vec::<String>::new();

        for violation in &diagnosis.violations {
            if let Some(mapping) = RepairMapping::from_rule(&violation.rule, false) {
                suggestions.push(mapping.suggestion(&violation.message));
                extend_unique_steps(&mut step_descriptions, mapping.steps);
            }
        }

        for warning in &diagnosis.warnings {
            if let Some(mapping) = RepairMapping::from_rule(&warning.rule, true) {
                suggestions.push(mapping.suggestion(&warning.message));
                extend_unique_steps(&mut step_descriptions, mapping.steps);
            }
        }

        let execution_steps: Vec<RepairStep> = step_descriptions
            .into_iter()
            .enumerate()
            .map(|(index, description)| RepairStep {
                order: index + 1,
                description,
            })
            .collect();

        if suggestions
            .iter()
            .any(|suggestion| suggestion.id.starts_with("ui-"))
        {
            crate::tui::render_trace::record("ui_repair_plan_generated");
        }

        RepairPlan {
            suggestions,
            execution_steps,
        }
    }
}

impl ImplementationPlanner {
    pub fn generate(repair_plan: &RepairPlan) -> ImplementationPlan {
        crate::tui::render_trace::record("implementation_planning_started");

        if repair_plan.suggestions.is_empty() {
            crate::tui::render_trace::record("implementation_planning_completed");
            return ImplementationPlan {
                tasks: Vec::new(),
                file_modifications: Vec::new(),
                validations: Vec::new(),
            };
        }

        let mut tasks = Vec::new();
        let mut file_modifications = Vec::new();
        let mut validations = Vec::new();

        for suggestion in &repair_plan.suggestions {
            if let Some(mapping) = ImplementationMapping::from_suggestion(suggestion) {
                push_unique_task(
                    &mut tasks,
                    ImplementationTask {
                        id: mapping.id.to_string(),
                        title: mapping.task_title.to_string(),
                        description: mapping.task_description.to_string(),
                        target_component: mapping.target_component.to_string(),
                        priority: implementation_priority(suggestion.priority),
                    },
                );
                crate::tui::render_trace::record("implementation_task_generated");

                if let Some(file_plan) = mapping.file_modification {
                    push_unique_file_plan(
                        &mut file_modifications,
                        FileModificationPlan {
                            target_file: file_plan.target_file.to_string(),
                            action: file_plan.action.to_string(),
                            rationale: file_plan.rationale.to_string(),
                        },
                    );
                    crate::tui::render_trace::record("file_modification_plan_generated");
                }

                for validation in mapping.validations {
                    push_unique_validation(
                        &mut validations,
                        ValidationPlan {
                            validation_type: validation.validation_type.to_string(),
                            description: validation.description.to_string(),
                        },
                    );
                    crate::tui::render_trace::record("validation_plan_generated");
                }
            }
        }

        for validation in mandatory_validations() {
            push_unique_validation(
                &mut validations,
                ValidationPlan {
                    validation_type: validation.validation_type.to_string(),
                    description: validation.description.to_string(),
                },
            );
            crate::tui::render_trace::record("validation_plan_generated");
        }

        crate::tui::render_trace::record("implementation_planning_completed");
        if repair_plan
            .suggestions
            .iter()
            .any(|suggestion| suggestion.id.starts_with("ui-"))
        {
            crate::tui::render_trace::record("ui_implementation_plan_generated");
        }

        ImplementationPlan {
            tasks,
            file_modifications,
            validations,
        }
    }
}

struct ImplementationMapping {
    id: &'static str,
    task_title: &'static str,
    task_description: &'static str,
    target_component: &'static str,
    file_modification: Option<FilePlanMapping>,
    validations: &'static [ValidationMapping],
}

#[derive(Clone, Copy)]
struct FilePlanMapping {
    target_file: &'static str,
    action: &'static str,
    rationale: &'static str,
}

#[derive(Clone, Copy)]
struct ValidationMapping {
    validation_type: &'static str,
    description: &'static str,
}

impl ImplementationMapping {
    fn from_suggestion(suggestion: &RepairSuggestion) -> Option<Self> {
        match suggestion.title.as_str() {
            "Introduce missing workspace" => Some(Self {
                id: "add-workspace-task",
                task_title: "Add WorkspaceState",
                task_description: "Plan missing workspace state and projection support for the UI architecture.",
                target_component: "workspace",
                file_modification: Some(FilePlanMapping {
                    target_file: "workspace.rs",
                    action: "Add missing workspace state",
                    rationale: "Required workspaces must be represented before rendering can expose them.",
                }),
                validations: &[ValidationMapping {
                    validation_type: "UIStructuralReDiagnosis",
                    description: "Workspace visibility re-run",
                }],
            }),
            "Create Pipeline Workspace" => Some(Self {
                id: "add-pipeline-workspace-task",
                task_title: "Add PipelineWorkspaceState",
                task_description: "Plan state, renderer, and projection routing for Recognition, Diagnosis, RepairPlan, and ImplementationPlan visibility.",
                target_component: "PipelineWorkspace",
                file_modification: Some(FilePlanMapping {
                    target_file: "state.rs",
                    action: "Add PipelineWorkspaceState",
                    rationale: "Pipeline visibility needs a dedicated state surface.",
                }),
                validations: &[ValidationMapping {
                    validation_type: "UIStructuralReDiagnosis",
                    description: "Pipeline visibility re-run",
                }],
            }),
            "Create Task Workspace" => Some(Self {
                id: "add-task-workspace-task",
                task_title: "Add TaskWorkspaceState",
                task_description: "Plan task state and renderer coverage for repair suggestions, implementation tasks, and validation tasks.",
                target_component: "ActiveTaskWorkspace",
                file_modification: Some(FilePlanMapping {
                    target_file: "workspace.rs",
                    action: "Add task workspace projection",
                    rationale: "Task visibility requires a workspace-level projection target.",
                }),
                validations: &[ValidationMapping {
                    validation_type: "UIStructuralReDiagnosis",
                    description: "Task visibility re-run",
                }],
            }),
            "Create Event Timeline" => Some(Self {
                id: "add-timeline-task",
                task_title: "Add TimelineRenderer",
                task_description: "Plan timeline rendering for runtime, diagnosis, and planning events.",
                target_component: "EventTimeline",
                file_modification: Some(FilePlanMapping {
                    target_file: "render.rs",
                    action: "Add TimelineRenderer",
                    rationale: "Event visibility needs an explicit timeline renderer.",
                }),
                validations: &[ValidationMapping {
                    validation_type: "UIStructuralReDiagnosis",
                    description: "Timeline visibility re-run",
                }],
            }),
            "Add Specification Editor" => Some(Self {
                id: "add-specification-editor-task",
                task_title: "Add SpecificationEditor",
                task_description: "Plan an accessible editor surface for entering and revising UI specifications.",
                target_component: "SpecificationEditor",
                file_modification: Some(FilePlanMapping {
                    target_file: "rendering/mod.rs",
                    action: "Add specification editor projection",
                    rationale: "UI diagnosis must expose the input surface it depends on.",
                }),
                validations: &[ValidationMapping {
                    validation_type: "UIStructuralReDiagnosis",
                    description: "Input accessibility re-run",
                }],
            }),
            "Merge related workspaces" => Some(Self {
                id: "rebalance-workspace-task",
                task_title: "Add WorkspaceMergePlan",
                task_description: "Plan consolidation of related workspaces to reduce cognitive load.",
                target_component: "workspace layout",
                file_modification: Some(FilePlanMapping {
                    target_file: "workspace.rs",
                    action: "Merge related workspace definitions",
                    rationale: "Too many workspaces increase navigation and scanning cost.",
                }),
                validations: &[ValidationMapping {
                    validation_type: "UIStructuralReDiagnosis",
                    description: "Cognitive load re-run",
                }],
            }),
            "Consolidate duplicate visualization" => Some(Self {
                id: "consolidate-visualization-task",
                task_title: "Add VisualizationOwnershipRoute",
                task_description: "Plan a single ownership route for duplicated information visualization.",
                target_component: "ProjectionRoute",
                file_modification: Some(FilePlanMapping {
                    target_file: "rendering/mod.rs",
                    action: "Add visualization ownership route",
                    rationale: "Duplicate information should be consolidated into one rendering owner.",
                }),
                validations: &[ValidationMapping {
                    validation_type: "UIStructuralReDiagnosis",
                    description: "Information duplication re-run",
                }],
            }),
            "Rebalance workspace ratio" => Some(Self {
                id: "add-layout-rebalance-task",
                task_title: "Add LayoutBalanceRule",
                task_description: "Plan layout constraints that keep workspace occupancy within the dashboard balance threshold.",
                target_component: "DashboardLayout",
                file_modification: Some(FilePlanMapping {
                    target_file: "render.rs",
                    action: "Rebalance workspace ratio",
                    rationale: "Dashboard layout must reserve space for context, tasks, and timeline surfaces.",
                }),
                validations: &[ValidationMapping {
                    validation_type: "UIStructuralReDiagnosis",
                    description: "Layout balance re-run",
                }],
            }),
            "Introduce AuditGateway" => Some(Self {
                id: "impl-audit-gateway",
                task_title: "Create AuditGateway abstraction",
                task_description: "Plan an audit boundary that routes runtime access through AuditCore without mutating source code in this phase.",
                target_component: "AuditCore",
                file_modification: Some(FilePlanMapping {
                    target_file: "audit_gateway.rs",
                    action: "Introduce AuditGateway abstraction",
                    rationale: "Make audit routing explicit before any implementation change is generated.",
                }),
                validations: &[ValidationMapping {
                    validation_type: "RuntimeValidation",
                    description: "Audit routing verification",
                }],
            }),
            "Enforce ApplyGate" => Some(Self {
                id: "impl-applygate",
                task_title: "Route mutation through ApplyGate",
                task_description: "Plan mutation routing through ApplyGate while keeping this phase limited to implementation planning.",
                target_component: "runtime layer",
                file_modification: Some(FilePlanMapping {
                    target_file: "runtime layer",
                    action: "Route execution through ApplyGate",
                    rationale: "Prevent direct mutation paths from bypassing the apply boundary.",
                }),
                validations: &[ValidationMapping {
                    validation_type: "IntegrationTest",
                    description: "ApplyGate integration test",
                }],
            }),
            "Separate responsibilities" => Some(Self {
                id: "impl-layer-boundary",
                task_title: "Introduce boundary interface",
                task_description: "Plan a boundary interface that separates layer responsibilities.",
                target_component: "layer boundary",
                file_modification: None,
                validations: &[ValidationMapping {
                    validation_type: "StructuralReDiagnosis",
                    description: "Dependency analysis",
                }],
            }),
            "Break dependency cycle" => Some(Self {
                id: "impl-break-cycle",
                task_title: "Extract shared interface",
                task_description: "Plan extraction of a shared interface to remove the dependency cycle.",
                target_component: "dependency graph",
                file_modification: None,
                validations: &[ValidationMapping {
                    validation_type: "StructuralReDiagnosis",
                    description: "Cycle detection re-run",
                }],
            }),
            _ => None,
        }
    }
}

struct RepairMapping {
    id: &'static str,
    title: &'static str,
    impact: RepairImpact,
    priority: RepairPriority,
    steps: &'static [&'static str],
}

impl RepairMapping {
    fn from_rule(rule: &str, warning: bool) -> Option<Self> {
        let normalized = rule.to_ascii_lowercase();
        let compact = normalized
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .collect::<String>();
        let mut mapping = if normalized.contains("auditcore")
            || normalized.contains("audit bypass")
            || normalized.contains("runtime bypass")
            || compact.contains("auditbypass")
        {
            Self {
                id: "audit-bypass",
                title: "Introduce AuditGateway",
                impact: RepairImpact::High,
                priority: RepairPriority::Critical,
                steps: &[
                    "Locate bypass path",
                    "Introduce AuditGateway",
                    "Redirect execution",
                ],
            }
        } else if compact.contains("workspacevisibilityviolation") {
            Self {
                id: "ui-workspace-repair",
                title: "Introduce missing workspace",
                impact: RepairImpact::High,
                priority: RepairPriority::Critical,
                steps: &[
                    "Identify missing workspace",
                    "Introduce missing workspace",
                    "Add workspace projection route",
                ],
            }
        } else if compact.contains("pipelinevisibilityviolation") {
            Self {
                id: "ui-pipeline-repair",
                title: "Create Pipeline Workspace",
                impact: RepairImpact::High,
                priority: RepairPriority::Critical,
                steps: &[
                    "Create Pipeline Workspace",
                    "Add PipelineRenderer",
                    "Add ProjectionRoute",
                ],
            }
        } else if compact.contains("taskvisibilityviolation") {
            Self {
                id: "ui-task-repair",
                title: "Create Task Workspace",
                impact: RepairImpact::High,
                priority: RepairPriority::Critical,
                steps: &[
                    "Create Task Workspace",
                    "Add task renderer",
                    "Add validation task projection",
                ],
            }
        } else if compact.contains("timelinevisibilityviolation") {
            Self {
                id: "ui-timeline-repair",
                title: "Create Event Timeline",
                impact: RepairImpact::High,
                priority: RepairPriority::Critical,
                steps: &[
                    "Create Event Timeline",
                    "Add TimelineRenderer",
                    "Project runtime, diagnosis, and planning events",
                ],
            }
        } else if compact.contains("inputaccessibilityviolation") {
            Self {
                id: "ui-input-repair",
                title: "Add Specification Editor",
                impact: RepairImpact::Medium,
                priority: RepairPriority::Critical,
                steps: &[
                    "Add Specification Editor",
                    "Connect editor input to specification context",
                    "Expose validation feedback",
                ],
            }
        } else if compact.contains("excessivecognitiveload") {
            Self {
                id: "ui-cognitive-load-repair",
                title: "Merge related workspaces",
                impact: RepairImpact::Medium,
                priority: RepairPriority::Recommended,
                steps: &[
                    "Group related workspaces",
                    "Merge related workspaces",
                    "Re-run UI diagnosis",
                ],
            }
        } else if compact.contains("informationduplicationviolation") {
            Self {
                id: "ui-duplication-repair",
                title: "Consolidate duplicate visualization",
                impact: RepairImpact::Medium,
                priority: RepairPriority::Recommended,
                steps: &[
                    "Identify duplicate visualization owner",
                    "Consolidate duplicate visualization",
                    "Update projection route",
                ],
            }
        } else if compact.contains("layoutbalanceviolation") {
            Self {
                id: "ui-layout-repair",
                title: "Rebalance workspace ratio",
                impact: RepairImpact::Medium,
                priority: RepairPriority::Recommended,
                steps: &[
                    "Measure workspace ratio",
                    "Rebalance workspace ratio",
                    "Validate dashboard layout",
                ],
            }
        } else if normalized.contains("applygate")
            || normalized.contains("memory write")
            || normalized.contains("direct mutation")
            || compact.contains("applygatemissing")
        {
            Self {
                id: "applygate-missing",
                title: "Enforce ApplyGate",
                impact: RepairImpact::High,
                priority: RepairPriority::Critical,
                steps: &[
                    "Detect direct mutation path",
                    "Replace with ApplyGate",
                    "Add validation test",
                ],
            }
        } else if normalized.contains("cyclic") || normalized.contains("cycle") {
            Self {
                id: "cyclic-dependency",
                title: "Break dependency cycle",
                impact: RepairImpact::Medium,
                priority: RepairPriority::Recommended,
                steps: &[
                    "Identify cycle",
                    "Extract shared interface",
                    "Redirect dependency",
                ],
            }
        } else if normalized.contains("layer") {
            Self {
                id: "layer-violation",
                title: "Separate responsibilities",
                impact: RepairImpact::Medium,
                priority: RepairPriority::Recommended,
                steps: &[
                    "Identify crossing dependency",
                    "Introduce boundary interface",
                    "Move dependency behind abstraction",
                ],
            }
        } else {
            return None;
        };

        if warning {
            mapping.priority = RepairPriority::Optional;
            mapping.impact = RepairImpact::Low;
        }

        Some(mapping)
    }

    fn suggestion(&self, rationale: &str) -> RepairSuggestion {
        RepairSuggestion {
            id: self.id.to_string(),
            title: self.title.to_string(),
            rationale: rationale.to_string(),
            impact: self.impact,
            priority: self.priority,
        }
    }
}

fn extend_unique_steps(target: &mut Vec<String>, steps: &[&str]) {
    for step in steps {
        if !target.iter().any(|existing| existing == step) {
            target.push((*step).to_string());
        }
    }
}

fn implementation_priority(priority: RepairPriority) -> ImplementationPriority {
    match priority {
        RepairPriority::Critical => ImplementationPriority::Critical,
        RepairPriority::Recommended => ImplementationPriority::Normal,
        RepairPriority::Optional => ImplementationPriority::Low,
    }
}

fn mandatory_validations() -> &'static [ValidationMapping] {
    &[
        ValidationMapping {
            validation_type: "UnitTest",
            description: "Implementation task generation unit test",
        },
        ValidationMapping {
            validation_type: "IntegrationTest",
            description: "ImplementationPlan REPL integration test",
        },
    ]
}

fn push_unique_task(target: &mut Vec<ImplementationTask>, task: ImplementationTask) {
    if !target.iter().any(|existing| existing.id == task.id) {
        target.push(task);
    }
}

fn push_unique_file_plan(target: &mut Vec<FileModificationPlan>, plan: FileModificationPlan) {
    if !target
        .iter()
        .any(|existing| existing.target_file == plan.target_file && existing.action == plan.action)
    {
        target.push(plan);
    }
}

fn push_unique_validation(target: &mut Vec<ValidationPlan>, plan: ValidationPlan) {
    if !target.iter().any(|existing| {
        existing.validation_type == plan.validation_type && existing.description == plan.description
    }) {
        target.push(plan);
    }
}

fn optional_string(
    value: Option<&Value>,
    field: &str,
) -> Result<Option<String>, SpecificationError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        Some(other) => Err(SpecificationError::InvalidStructure(format!(
            "{field} must be a string, got {}",
            value_kind(other)
        ))),
    }
}

fn string_list(value: Option<&Value>, field: &str) -> Result<Vec<String>, SpecificationError> {
    match value {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Sequence(values)) => values
            .iter()
            .map(|value| {
                value.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                    SpecificationError::InvalidStructure(format!("{field} items must be strings"))
                })
            })
            .collect(),
        Some(Value::String(value)) if value.trim().is_empty() => Ok(Vec::new()),
        Some(Value::String(value)) => Ok(vec![value.clone()]),
        Some(other) => Err(SpecificationError::InvalidStructure(format!(
            "{field} must be a string list, got {}",
            value_kind(other)
        ))),
    }
}

fn component_list(value: Option<&Value>) -> Result<Vec<ComponentSpec>, SpecificationError> {
    match value {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Sequence(values)) => values.iter().map(component_from_value).collect(),
        Some(Value::Mapping(map)) => map
            .iter()
            .map(|(key, value)| {
                let name = key.as_str().ok_or_else(|| {
                    SpecificationError::InvalidStructure(
                        "architecture component key must be a string".into(),
                    )
                })?;
                let responsibilities = match value {
                    Value::Null => Vec::new(),
                    Value::Sequence(_) | Value::String(_) => {
                        string_list(Some(value), "responsibilities")?
                    }
                    Value::Mapping(component_map) => string_list(
                        component_map.get(Value::String("responsibilities".into())),
                        "responsibilities",
                    )?,
                    other => {
                        return Err(SpecificationError::InvalidStructure(format!(
                            "architecture component must be a mapping or string list, got {}",
                            value_kind(other)
                        )));
                    }
                };
                Ok(ComponentSpec {
                    name: name.to_string(),
                    responsibilities,
                })
            })
            .collect(),
        Some(other) => Err(SpecificationError::InvalidStructure(format!(
            "architecture must be a component list, got {}",
            value_kind(other)
        ))),
    }
}

fn component_from_value(value: &Value) -> Result<ComponentSpec, SpecificationError> {
    let map = value.as_mapping().ok_or_else(|| {
        SpecificationError::InvalidStructure("architecture items must be mappings".into())
    })?;
    let name = map
        .get(Value::String("name".into()))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            SpecificationError::InvalidStructure("architecture item missing string name".into())
        })?;
    let responsibilities = string_list(
        map.get(Value::String("responsibilities".into())),
        "responsibilities",
    )?;
    Ok(ComponentSpec {
        name: name.to_string(),
        responsibilities,
    })
}

fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Sequence(_) => "sequence",
        Value::Mapping(_) => "mapping",
        Value::Tagged(_) => "tagged",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifier_yaml_design_spec() {
        let text = "system_name: DBM\ngoals:\n  - diagnose\n";
        assert_eq!(
            classify_specification(text),
            SpecificationKind::DesignSpecification
        );
    }

    #[test]
    fn classifier_instruction_spec() {
        let text = "Title: DBM-A\nGoal: X\nDeliverables:\n  - Y\n";
        assert_eq!(classify_specification(text), SpecificationKind::Instruction);
    }

    #[test]
    fn recognizer_primary_keys_start_design_specification() {
        for line in [
            "system_name: DBM",
            "goals:",
            "constraints:",
            "architecture:",
            "rules:",
        ] {
            assert!(
                DesignSpecificationRecognizer::is_design_specification_start(line),
                "{line}"
            );
        }
    }

    #[test]
    fn recognizer_secondary_keys_start_design_specification() {
        for line in [
            "components:",
            "interfaces:",
            "dependencies:",
            "layers:",
            "security:",
            "memory:",
            "runtime:",
            "audit:",
        ] {
            assert!(
                DesignSpecificationRecognizer::is_design_specification_start(line),
                "{line}"
            );
        }
    }

    #[test]
    fn recognizer_rejects_non_top_level_yaml_keys() {
        for line in [
            "let system_name = \"dbm\";",
            "fn goals() {}",
            "constraints.rs",
            "  system_name: DBM",
            "system_name : DBM",
        ] {
            assert!(
                !DesignSpecificationRecognizer::is_design_specification_start(line),
                "{line}"
            );
        }
    }

    #[test]
    fn context_valid_yaml_parse_success() {
        let text = r#"
system_name: DBM
goals:
  - diagnose design
constraints:
  - no bypass
architecture:
  - name: Runtime
    responsibilities:
      - dispatch via AuditCore
rules:
  - ApplyGate required
"#;
        let context = SpecificationContext::from_yaml(text).expect("parse");
        assert_eq!(context.system_name.as_deref(), Some("DBM"));
        assert_eq!(context.goals.len(), 1);
        assert_eq!(context.constraints.len(), 1);
        assert_eq!(context.architecture.len(), 1);
        assert_eq!(context.rules.len(), 1);
    }

    #[test]
    fn context_missing_fields_parse_success() {
        let context = SpecificationContext::from_yaml("system_name: DBM\n").expect("parse");
        assert_eq!(context.system_name.as_deref(), Some("DBM"));
        assert!(context.goals.is_empty());
        assert!(context.constraints.is_empty());
        assert!(context.architecture.is_empty());
        assert!(context.rules.is_empty());
    }

    #[test]
    fn context_invalid_structure_parse_failure() {
        let err = SpecificationContext::from_yaml("architecture: 1\n").expect_err("invalid");
        assert!(matches!(err, SpecificationError::InvalidStructure(_)));
    }

    #[test]
    fn structural_diagnosis_request_generation_success() {
        let context = SpecificationContext::from_yaml(
            "system_name: DBM\narchitecture:\nrules:\n  - ApplyGate required\n",
        )
        .expect("parse");
        let request = StructuralDiagnosisRequest::new(context);
        let result = request.diagnose();
        assert!(result.violations.is_empty());
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn ui_specification_auto_selects_user_interface_domain() {
        let context = SpecificationContext::from_yaml(
            r#"
system_name: DBM_TUI_PHASE3
architecture:
  DesignWorkspace:
  ActiveTaskWorkspace:
"#,
        )
        .expect("parse");

        let request = StructuralDiagnosisRequest::new(context);

        assert_eq!(request.domain, DiagnosisDomain::UserInterface);
    }

    #[test]
    fn domain_trace_selects_core_architecture_by_default() {
        let context = SpecificationContext::from_yaml("system_name: CoreSystem\n").expect("parse");

        let request = StructuralDiagnosisRequest::new(context);

        assert_eq!(request.domain, DiagnosisDomain::CoreArchitecture);
        assert_eq!(request.domain_reason, DomainReason::DefaultCoreArchitecture);
        assert_eq!(request.domain.as_str(), "CoreArchitecture");
        assert_eq!(request.domain.diagnosis_log_label(), "CORE_DIAGNOSIS");
    }

    #[test]
    fn domain_trace_selects_runtime_safety_for_runtime_keywords() {
        let context = SpecificationContext::from_yaml(
            r#"
system_name: RuntimeControl
rules:
  - ApplyGate required
  - AuditGateway required
"#,
        )
        .expect("parse");

        let request = StructuralDiagnosisRequest::new(context);

        assert_eq!(request.domain, DiagnosisDomain::RuntimeSafety);
        assert_eq!(
            request.domain_reason,
            DomainReason::RuntimeSafetyKeywordDetected
        );
        assert_eq!(request.domain.as_str(), "RuntimeSafety");
        assert_eq!(request.domain.diagnosis_log_label(), "RUNTIME_DIAGNOSIS");
    }

    #[test]
    fn domain_trace_selects_user_interface_for_ui_threshold() {
        let context = SpecificationContext::from_yaml(
            r#"
system_name: DBM_TUI
architecture:
  DesignWorkspace:
  PipelineWorkspace:
  EventTimeline:
"#,
        )
        .expect("parse");

        let request = StructuralDiagnosisRequest::new(context);

        assert_eq!(request.domain, DiagnosisDomain::UserInterface);
        assert_eq!(
            request.domain_reason,
            DomainReason::UiKeywordThresholdExceeded
        );
        assert_eq!(request.domain.as_str(), "UserInterface");
        assert_eq!(request.domain.diagnosis_log_label(), "UI_DIAGNOSIS");
    }

    #[test]
    fn ui_workspace_missing_diagnosis() {
        let context = SpecificationContext::from_yaml(
            r#"
architecture:
  PipelineWorkspace:
"#,
        )
        .expect("parse");

        let diagnosis =
            StructuralDiagnosisRequest::with_domain(context, DiagnosisDomain::UserInterface)
                .diagnose();

        assert!(diagnosis.violations.iter().any(|violation| {
            violation.rule == "WorkspaceVisibilityViolation"
                && violation.message.contains("DesignWorkspace")
                && violation.message.contains("ActiveTaskWorkspace")
        }));
    }

    #[test]
    fn ui_missing_timeline_diagnosis() {
        let context = SpecificationContext::from_yaml(
            r#"
architecture:
  DesignWorkspace:
"#,
        )
        .expect("parse");

        let diagnosis =
            StructuralDiagnosisRequest::with_domain(context, DiagnosisDomain::UserInterface)
                .diagnose();

        assert!(
            diagnosis
                .violations
                .iter()
                .any(|violation| violation.rule == "TimelineVisibilityViolation")
        );
    }

    #[test]
    fn ui_cognitive_load_warning() {
        let context = SpecificationContext::from_yaml(
            r#"
architecture:
  Workspace1:
  Workspace2:
  Workspace3:
  Workspace4:
  Workspace5:
  Workspace6:
  Workspace7:
  Workspace8:
  Workspace9:
  Workspace10:
"#,
        )
        .expect("parse");

        let diagnosis =
            StructuralDiagnosisRequest::with_domain(context, DiagnosisDomain::UserInterface)
                .diagnose();

        assert!(
            diagnosis
                .warnings
                .iter()
                .any(|warning| warning.rule == "ExcessiveCognitiveLoad")
        );
    }

    #[test]
    fn ui_repair_plan_generated() {
        let diagnosis = StructuralDiagnosisResult {
            violations: vec![Violation {
                rule: "PipelineVisibilityViolation".into(),
                message: "missing pipeline visualization responsibility".into(),
            }],
            warnings: vec![],
        };

        let plan = RepairPlanner::generate(&diagnosis);

        assert!(
            plan.suggestions
                .iter()
                .any(|suggestion| suggestion.title == "Create Pipeline Workspace")
        );
    }

    #[test]
    fn ui_implementation_plan_generated() {
        let repair_plan = RepairPlan {
            suggestions: vec![RepairSuggestion {
                id: "ui-pipeline-repair".into(),
                title: "Create Pipeline Workspace".into(),
                rationale: "missing pipeline visualization responsibility".into(),
                impact: RepairImpact::High,
                priority: RepairPriority::Critical,
            }],
            execution_steps: vec![],
        };

        let plan = ImplementationPlanner::generate(&repair_plan);

        assert!(
            plan.tasks
                .iter()
                .any(|task| task.title == "Add PipelineWorkspaceState")
        );
        assert!(
            plan.file_modifications
                .iter()
                .any(|file_plan| file_plan.target_file == "state.rs")
        );
    }

    #[test]
    fn ui_e2e_pipeline_generates_repair_and_implementation() {
        let context = SpecificationContext::from_yaml(
            r#"
system_name: DBM_TUI_PHASE3
architecture:
  DesignWorkspace:
  ActiveTaskWorkspace:
"#,
        )
        .expect("parse");

        let diagnosis = StructuralDiagnosisRequest::new(context).diagnose();
        let repair_plan = RepairPlanner::generate(&diagnosis);
        let implementation_plan = ImplementationPlanner::generate(&repair_plan);

        assert!(
            diagnosis
                .violations
                .iter()
                .any(|violation| violation.rule == "PipelineVisibilityViolation")
        );
        assert!(
            diagnosis
                .violations
                .iter()
                .any(|violation| violation.rule == "TimelineVisibilityViolation")
        );
        assert!(
            repair_plan
                .suggestions
                .iter()
                .any(|suggestion| suggestion.title == "Create Pipeline Workspace")
        );
        assert!(
            repair_plan
                .suggestions
                .iter()
                .any(|suggestion| suggestion.title == "Create Event Timeline")
        );
        assert!(
            implementation_plan
                .tasks
                .iter()
                .any(|task| task.title == "Add PipelineWorkspaceState")
        );
        assert!(
            implementation_plan
                .tasks
                .iter()
                .any(|task| task.title == "Add TimelineRenderer")
        );
    }

    #[test]
    fn repair_suggestion_generation_audit_bypass() {
        let diagnosis = StructuralDiagnosisResult {
            violations: vec![Violation {
                rule: "AuditBypass".into(),
                message: "Execution path bypasses AuditCore".into(),
            }],
            warnings: vec![],
        };

        let plan = RepairPlanner::generate(&diagnosis);

        assert_eq!(plan.suggestions.len(), 1);
        assert_eq!(plan.suggestions[0].title, "Introduce AuditGateway");
        assert_eq!(plan.suggestions[0].priority, RepairPriority::Critical);
        assert!(
            plan.execution_steps
                .iter()
                .any(|step| step.description == "Introduce AuditGateway")
        );
    }

    #[test]
    fn repair_plan_generation_applygate_missing() {
        let diagnosis = StructuralDiagnosisResult {
            violations: vec![Violation {
                rule: "ApplyGateMissing".into(),
                message: "Direct mutation path detected".into(),
            }],
            warnings: vec![],
        };

        let plan = RepairPlanner::generate(&diagnosis);

        assert_eq!(plan.suggestions.len(), 1);
        assert_eq!(plan.suggestions[0].title, "Enforce ApplyGate");
        assert!(
            plan.execution_steps
                .iter()
                .any(|step| step.description == "Replace with ApplyGate")
        );
    }

    #[test]
    fn repair_plan_multi_violation_unifies_duplicate_steps() {
        let diagnosis = StructuralDiagnosisResult {
            violations: vec![
                Violation {
                    rule: "AuditBypass".into(),
                    message: "Execution path bypasses AuditCore".into(),
                },
                Violation {
                    rule: "ApplyGateMissing".into(),
                    message: "Direct mutation path detected".into(),
                },
                Violation {
                    rule: "AuditBypass".into(),
                    message: "Second bypass path".into(),
                },
            ],
            warnings: vec![],
        };

        let plan = RepairPlanner::generate(&diagnosis);

        assert_eq!(plan.suggestions.len(), 3);
        let redirect_count = plan
            .execution_steps
            .iter()
            .filter(|step| step.description == "Redirect execution")
            .count();
        assert_eq!(redirect_count, 1);
    }

    #[test]
    fn repair_plan_empty_diagnosis_is_empty() {
        let diagnosis = StructuralDiagnosisResult {
            violations: vec![],
            warnings: vec![],
        };

        let plan = RepairPlanner::generate(&diagnosis);

        assert!(plan.suggestions.is_empty());
        assert!(plan.execution_steps.is_empty());
    }

    #[test]
    fn repair_plan_warning_only_is_optional() {
        let diagnosis = StructuralDiagnosisResult {
            violations: vec![],
            warnings: vec![Warning {
                rule: "LayerViolation".into(),
                message: "architecture mentions layer crossing".into(),
            }],
        };

        let plan = RepairPlanner::generate(&diagnosis);

        assert_eq!(plan.suggestions.len(), 1);
        assert_eq!(plan.suggestions[0].priority, RepairPriority::Optional);
    }

    #[test]
    fn implementation_plan_generation_applygate() {
        let repair_plan = RepairPlan {
            suggestions: vec![RepairSuggestion {
                id: "applygate-missing".into(),
                title: "Enforce ApplyGate".into(),
                rationale: "Direct mutation path detected".into(),
                impact: RepairImpact::High,
                priority: RepairPriority::Critical,
            }],
            execution_steps: vec![],
        };

        let plan = ImplementationPlanner::generate(&repair_plan);

        assert!(
            plan.tasks
                .iter()
                .any(|task| task.title == "Route mutation through ApplyGate")
        );
        assert!(
            plan.validations
                .iter()
                .any(|validation| validation.validation_type == "IntegrationTest")
        );
    }

    #[test]
    fn implementation_plan_generation_audit_gateway() {
        let repair_plan = RepairPlan {
            suggestions: vec![RepairSuggestion {
                id: "audit-bypass".into(),
                title: "Introduce AuditGateway".into(),
                rationale: "Execution path bypasses AuditCore".into(),
                impact: RepairImpact::High,
                priority: RepairPriority::Critical,
            }],
            execution_steps: vec![],
        };

        let plan = ImplementationPlanner::generate(&repair_plan);

        assert!(
            plan.tasks
                .iter()
                .any(|task| task.title == "Create AuditGateway abstraction")
        );
        assert!(
            plan.file_modifications
                .iter()
                .any(|file_plan| file_plan.target_file == "audit_gateway.rs")
        );
    }

    #[test]
    fn implementation_plan_multi_suggestion_is_unified() {
        let repair_plan = RepairPlan {
            suggestions: vec![
                RepairSuggestion {
                    id: "audit-bypass".into(),
                    title: "Introduce AuditGateway".into(),
                    rationale: "Execution path bypasses AuditCore".into(),
                    impact: RepairImpact::High,
                    priority: RepairPriority::Critical,
                },
                RepairSuggestion {
                    id: "applygate-missing".into(),
                    title: "Enforce ApplyGate".into(),
                    rationale: "Direct mutation path detected".into(),
                    impact: RepairImpact::High,
                    priority: RepairPriority::Critical,
                },
            ],
            execution_steps: vec![],
        };

        let plan = ImplementationPlanner::generate(&repair_plan);

        assert_eq!(plan.tasks.len(), 2);
        assert!(
            plan.file_modifications
                .iter()
                .any(|file_plan| file_plan.target_file == "audit_gateway.rs")
        );
        assert!(
            plan.file_modifications
                .iter()
                .any(|file_plan| file_plan.target_file == "runtime layer")
        );
        assert_eq!(
            plan.validations
                .iter()
                .filter(|validation| validation.validation_type == "UnitTest")
                .count(),
            1
        );
    }

    #[test]
    fn implementation_plan_empty_repair_plan_is_empty() {
        let repair_plan = RepairPlan {
            suggestions: vec![],
            execution_steps: vec![],
        };

        let plan = ImplementationPlanner::generate(&repair_plan);

        assert!(plan.tasks.is_empty());
        assert!(plan.file_modifications.is_empty());
        assert!(plan.validations.is_empty());
    }
}
