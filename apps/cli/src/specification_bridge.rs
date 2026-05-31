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

pub struct RepairPlanner;

impl StructuralDiagnosisRequest {
    pub fn new(specification: SpecificationContext) -> Self {
        Self { specification }
    }

    pub fn diagnose(&self) -> StructuralDiagnosisResult {
        run_structural_diagnosis(&self.specification)
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

        let execution_steps = step_descriptions
            .into_iter()
            .enumerate()
            .map(|(index, description)| RepairStep {
                order: index + 1,
                description,
            })
            .collect();

        RepairPlan {
            suggestions,
            execution_steps,
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
}
