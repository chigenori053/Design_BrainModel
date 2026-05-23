use std::path::PathBuf;

use crate::nl::intent::primary_intent;
use crate::nl::types::{
    CommandPlan, ExecutionPlan, IntentType, Operation, PlanSource, PlannedStep,
};
use crate::plan::{CommandInvocation, Plan, PlanStatus, Step};
use crate::session::AgentSession;

/// Converts IR CommandPlan to runtime Plan (for display).
pub fn to_legacy_plan(command_plan: &CommandPlan) -> Plan {
    let mut steps = Vec::new();
    for (index, ir_step) in command_plan.steps.iter().enumerate() {
        let (description, command) = match ir_step {
            PlannedStep::Analyze(path) => (
                format!("analyze: {}", path.display()),
                Some(CommandInvocation {
                    name: "analyze".to_string(),
                    subcommand: None,
                    args: vec![path.display().to_string()],
                }),
            ),
            PlannedStep::Coding(path, _) => (
                format!("coding: {}", path.display()),
                Some(CommandInvocation {
                    name: "coding".to_string(),
                    subcommand: None,
                    args: vec![path.display().to_string()],
                }),
            ),
            PlannedStep::Validate(path) => (
                format!("validate: {}", path.display()),
                Some(CommandInvocation {
                    name: "validate".to_string(),
                    subcommand: None,
                    args: vec![path.display().to_string()],
                }),
            ),
            PlannedStep::StructureView(path)
            | PlannedStep::StructureEdit(path)
            | PlannedStep::StructureUndo(path)
            | PlannedStep::StructureRedo(path)
            | PlannedStep::Run(path)
            | PlannedStep::Memory(path)
            | PlannedStep::GitCommit(path)
            | PlannedStep::GitPR(path)
            | PlannedStep::IrReload(path)
            | PlannedStep::IrReloadAll(path)
            | PlannedStep::ShowDeps(path) => (
                format!(
                    "{}: {}",
                    format!("{ir_step:?}").to_lowercase(),
                    path.display()
                ),
                Some(CommandInvocation {
                    name: format!("{ir_step:?}").to_lowercase(),
                    subcommand: None,
                    args: vec![path.display().to_string()],
                }),
            ),
            PlannedStep::StructureDiff(path, node) => (
                format!("structure diff: {}", path.display()),
                Some(CommandInvocation {
                    name: "structure_diff".to_string(),
                    subcommand: None,
                    args: node
                        .iter()
                        .cloned()
                        .chain(std::iter::once(path.display().to_string()))
                        .collect(),
                }),
            ),
            PlannedStep::Rules => (
                "rules".to_string(),
                Some(CommandInvocation {
                    name: "rules".to_string(),
                    subcommand: None,
                    args: Vec::new(),
                }),
            ),
            PlannedStep::AlternativeMutationSearch(spec)
            | PlannedStep::DesignDeltaReasoning(spec)
            | PlannedStep::ExplainDesignTradeoff(spec) => (
                spec.clone(),
                Some(CommandInvocation {
                    name: format!("{ir_step:?}").to_lowercase(),
                    subcommand: None,
                    args: vec![spec.clone()],
                }),
            ),
            PlannedStep::ApplyPreviousCodingStep => (
                "apply previous coding".to_string(),
                Some(CommandInvocation {
                    name: "apply_previous_coding".to_string(),
                    subcommand: None,
                    args: Vec::new(),
                }),
            ),
            PlannedStep::RollbackCurrentTransaction => (
                "rollback current transaction".to_string(),
                Some(CommandInvocation {
                    name: "rollback_current_transaction".to_string(),
                    subcommand: None,
                    args: Vec::new(),
                }),
            ),
            PlannedStep::Refactor(spec) => (
                format!("refactor: {}", spec.target.display()),
                Some(CommandInvocation {
                    name: "refactor".to_string(),
                    subcommand: None,
                    args: vec![spec.target.display().to_string(), spec.request.clone()],
                }),
            ),
            PlannedStep::Repair(spec) => (
                format!("repair: {}", spec.target.display()),
                Some(CommandInvocation {
                    name: "repair".to_string(),
                    subcommand: None,
                    args: vec![spec.target.display().to_string()],
                }),
            ),
            PlannedStep::Apply => (
                "apply IR transaction".to_string(),
                Some(CommandInvocation {
                    name: "apply".to_string(),
                    subcommand: None,
                    args: Vec::new(),
                }),
            ),
            PlannedStep::Reload => (
                "reload IR".to_string(),
                Some(CommandInvocation {
                    name: "reload".to_string(),
                    subcommand: None,
                    args: Vec::new(),
                }),
            ),
        };
        steps.push(Step::new(index, description, command));
    }

    let mut plan = Plan::new("ir-plan", steps);
    plan.status = PlanStatus::Ready;
    plan
}

/// ExecutionPlan → runtime Plan（UI表示用）
pub fn to_runtime_plan(plan: &ExecutionPlan) -> Plan {
    let target_str = plan
        .target
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| ".".to_string());
    let op_name = operation_name(&plan.operation);
    let command = Some(CommandInvocation {
        name: op_name.to_string(),
        subcommand: None,
        args: plan
            .target
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
    });
    let step = Step::new(0, format!("{op_name}: {target_str}"), command);
    let mut runtime_plan = Plan::new("exec-plan", vec![step]);
    runtime_plan.status = PlanStatus::Ready;
    runtime_plan
}

fn operation_name(op: &Operation) -> &'static str {
    match op {
        Operation::Analyze => "analyze",
        Operation::Refactor => "refactor",
        Operation::Validate => "validate",
        Operation::Composite(_) => "composite",
        Operation::Apply => "apply",
        Operation::Rollback => "rollback",
        Operation::Reload => "reload",
        Operation::Repair => "repair",
        Operation::NoOp => "noop",
    }
}

/// Structured representation of a long-form instruction.
///
/// Produced by [`InstructionPlan::from_spec`] when a multiline spec is
/// submitted via the REPL's `/begin spec … /end` capture mode.  The raw text
/// is parsed into five semantic fields so the mutation pipeline receives
/// structured intent rather than unprocessed prose.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstructionPlan {
    pub summary: String,
    pub target: Option<PathBuf>,
    pub operations: Vec<String>,
    pub risks: Vec<String>,
    pub validation_plan: Vec<String>,
}

impl InstructionPlan {
    /// Parse a multiline spec string into structured fields.
    pub fn from_spec(input: &str) -> Self {
        use crate::nl::normalization::normalize_runtime_input;
        use crate::nl::runtime_intent::MutationOperation;

        let plan_lines = split_plan_lines(input);
        let operation_input = plan_lines.operation_input();
        let normalized = normalize_runtime_input(&operation_input);
        let target = normalized.as_ref().and_then(|n| n.command.target.clone());
        let operations = normalized
            .as_ref()
            .map(|n| {
                n.command
                    .operations
                    .iter()
                    .map(|op| match op {
                        MutationOperation::Modify => "Modify".to_string(),
                        MutationOperation::AddConst { name } => format!("AddConst: {name}"),
                        MutationOperation::InsertLine { text } => {
                            if is_comment_instruction(text) {
                                format!("InsertComment: {}", plan_truncate(text, 60))
                            } else {
                                format!("Insert: {}", plan_truncate(text, 60))
                            }
                        }
                        MutationOperation::ReplaceBlock { text } => {
                            format!("Replace: {}", plan_truncate(text, 60))
                        }
                        MutationOperation::DeleteLine { text } => {
                            format!("Delete: {}", plan_truncate(text, 60))
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let summary = build_plan_summary(&plan_lines.summary_input(), &target, &operations);
        let risks = plan_lines.risks;
        let validation_plan = plan_lines.validation;

        Self {
            summary,
            target,
            operations,
            risks,
            validation_plan,
        }
    }

    /// Render the plan as `[PLAN]`-prefixed lines suitable for REPL output.
    pub fn render_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(format!("[PLAN] summary: {}", self.summary));
        match &self.target {
            Some(t) => lines.push(format!("[PLAN] target: {}", t.display())),
            None => lines.push("[PLAN] target: (none)".to_string()),
        }
        if self.operations.is_empty() {
            lines.push("[PLAN] operations: (none)".to_string());
        } else {
            for op in &self.operations {
                lines.push(format!("[PLAN] operation: {op}"));
            }
        }
        for risk in &self.risks {
            lines.push(format!("[PLAN] risk: {risk}"));
        }
        for step in &self.validation_plan {
            lines.push(format!("[PLAN] validate: {step}"));
        }
        lines
    }
}

#[derive(Debug, Default)]
struct ClassifiedPlanLines {
    target: Vec<String>,
    body: Vec<String>,
    risks: Vec<String>,
    validation: Vec<String>,
}

impl ClassifiedPlanLines {
    fn operation_input(&self) -> String {
        self.target
            .iter()
            .chain(self.body.iter())
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn summary_input(&self) -> String {
        self.body.join("\n")
    }
}

fn split_plan_lines(input: &str) -> ClassifiedPlanLines {
    let mut lines = ClassifiedPlanLines::default();
    for line in input.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if is_plan_validation_line(line) {
            lines.validation.push(line.to_string());
        } else if is_plan_risk_line(line) {
            lines.risks.push(line.to_string());
        } else if is_target_header_line(line) {
            lines.target.push(line.to_string());
        } else {
            lines.body.push(line.to_string());
        }
    }
    lines
}

fn plan_truncate(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
}

fn is_target_header_line(line: &str) -> bool {
    line.starts_with("Target:") || line.starts_with("target:") || line.starts_with("対象:")
}

fn is_plan_risk_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let trimmed = line.trim();
    lower.starts_with("note:")
        || lower.starts_with("warning:")
        || lower.starts_with("risk:")
        || trimmed.starts_with("注意:")
        || trimmed.starts_with("危険:")
        || lower.contains("careful")
        || lower.contains("caution")
        || lower.contains("danger")
        || lower.contains("breaking change")
        || lower.contains("might break")
        || lower.contains("注意")
        || lower.contains("危険")
}

fn is_plan_validation_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    let trimmed = line.trim();
    lower.starts_with("test:")
        || lower.starts_with("validation:")
        || lower.starts_with("validate:")
        || lower.starts_with("check:")
        || lower.starts_with("cargo test")
        || lower.starts_with("cargo check")
        || lower.starts_with("cargo clippy")
        || lower.starts_with("cargo fmt")
        || trimmed.starts_with("確認:")
        || lower.contains("cargo test")
        || lower.contains("cargo check")
        || lower.contains("cargo clippy")
        || lower.contains("cargo fmt")
}

fn is_comment_instruction(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    (lower.contains("comment") || line.contains("コメント"))
        && (lower.contains("add")
            || lower.contains("insert")
            || line.contains("追加")
            || line.contains("挿入"))
}

fn build_plan_summary(input: &str, target: &Option<PathBuf>, operations: &[String]) -> String {
    // Use the first non-empty, non-header, non-risk, non-validation line as summary.
    let desc = input.lines().map(str::trim).find(|line| {
        !line.is_empty()
            && !is_target_header_line(line)
            && !is_plan_risk_line(line)
            && !is_plan_validation_line(line)
    });
    if let Some(d) = desc {
        return plan_truncate(d, 80);
    }
    match (target.as_ref(), operations.first()) {
        (Some(t), Some(op)) => format!("{}: {op}", t.display()),
        (Some(t), None) => format!("Modify {}", t.display()),
        (None, Some(op)) => op.clone(),
        (None, None) => "(no actionable instruction)".to_string(),
    }
}

/// Legacy v1 planner（後方互換）
pub fn plan_input(input: &str, _session: &AgentSession) -> Option<ExecutionPlan> {
    let intent = primary_intent(input);
    match intent {
        IntentType::Analyze | IntentType::AnalyzeArchitecture => Some(ExecutionPlan::new(
            Operation::Analyze,
            Some(PathBuf::from(".")),
            PlanSource::System,
        )),
        IntentType::Coding | IntentType::CodingEdit => Some(
            ExecutionPlan::new(
                Operation::Refactor,
                Some(PathBuf::from(".")),
                PlanSource::System,
            )
            .with_query(input),
        ),
        _ => None,
    }
}

#[cfg(test)]
mod instruction_plan_tests {
    use super::*;

    #[test]
    fn instruction_plan_from_spec_with_target_header() {
        let input = "Target: apps/cli/src/repl.rs\nAdd multiline capture support.";
        let plan = InstructionPlan::from_spec(input);

        assert_eq!(plan.target, Some(PathBuf::from("apps/cli/src/repl.rs")));
        assert!(
            !plan.operations.is_empty(),
            "operations must be extracted from body"
        );
        assert!(plan.risks.is_empty());
        assert!(plan.validation_plan.is_empty());
    }

    #[test]
    fn instruction_plan_from_spec_extracts_risks() {
        let input =
            "Target: apps/cli/src/repl.rs\nAdd feature.\nNote: Be careful not to break tests.";
        let plan = InstructionPlan::from_spec(input);

        assert_eq!(plan.risks.len(), 1, "{:?}", plan.risks);
        assert!(plan.risks[0].contains("careful"), "{:?}", plan.risks);
    }

    #[test]
    fn instruction_plan_from_spec_extracts_validation() {
        let input = "Target: apps/cli/src/repl.rs\nAdd feature.\nTest: cargo test -p design_cli";
        let plan = InstructionPlan::from_spec(input);

        assert_eq!(plan.validation_plan.len(), 1, "{:?}", plan.validation_plan);
        assert!(
            plan.validation_plan[0].contains("cargo test"),
            "{:?}",
            plan.validation_plan
        );
    }

    #[test]
    fn instruction_plan_render_includes_all_sections() {
        let input = "Target: apps/cli/src/repl.rs\n\
                     Add multiline capture.\n\
                     Note: Careful with borrow checker.\n\
                     Test: cargo test -p design_cli";
        let plan = InstructionPlan::from_spec(input);
        let rendered = plan.render_lines().join("\n");

        assert!(rendered.contains("[PLAN] summary:"), "{rendered}");
        assert!(
            rendered.contains("[PLAN] target: apps/cli/src/repl.rs"),
            "{rendered}"
        );
        assert!(rendered.contains("[PLAN] operation:"), "{rendered}");
        assert!(rendered.contains("[PLAN] risk:"), "{rendered}");
        assert!(rendered.contains("[PLAN] validate:"), "{rendered}");
    }

    #[test]
    fn instruction_plan_no_target_renders_none() {
        let input = "Add something somewhere.";
        let plan = InstructionPlan::from_spec(input);
        let rendered = plan.render_lines().join("\n");

        assert!(rendered.contains("[PLAN] target: (none)"), "{rendered}");
    }

    #[test]
    fn instruction_plan_summary_uses_first_description_line() {
        let input = "Target: apps/cli/src/repl.rs\nThis adds the capture buffer.\nNote: dangerous.";
        let plan = InstructionPlan::from_spec(input);

        assert_eq!(plan.summary, "This adds the capture buffer.");
    }

    #[test]
    fn instruction_plan_risks_and_validation_not_in_operations() {
        let input = "Target: apps/cli/src/repl.rs\n\
                     Add capture.\n\
                     Note: Be careful.\n\
                     Test: cargo check\n\
                     Validation: cargo clippy";
        let plan = InstructionPlan::from_spec(input);

        // Risk/validation lines must not leak into operations
        for op in &plan.operations {
            assert!(!op.contains("careful"), "risk leaked into operation: {op}");
            assert!(
                !op.contains("cargo"),
                "validation leaked into operation: {op}"
            );
        }
        assert!(!plan.risks.is_empty(), "risks must be detected");
        assert!(
            !plan.validation_plan.is_empty(),
            "validation must be detected"
        );
    }

    #[test]
    fn instruction_plan_excludes_risk_from_operations() {
        let input = "Target: apps/cli/src/repl.rs\n\
                     REPL long instruction flow の手動E2E確認用コメントを追加する。\n\
                     Risk: 不要な大規模変更をしない。";
        let plan = InstructionPlan::from_spec(input);

        assert!(
            plan.operations
                .iter()
                .all(|operation| !operation.contains("Risk:")),
            "{:?}",
            plan.operations
        );
        assert_eq!(plan.risks, vec!["Risk: 不要な大規模変更をしない。"]);
    }

    #[test]
    fn instruction_plan_excludes_validation_from_operations() {
        let input = "Target: apps/cli/src/repl.rs\n\
                     REPL long instruction flow の手動E2E確認用コメントを追加する。\n\
                     Validation: cargo test -p design_cli --test integration";
        let plan = InstructionPlan::from_spec(input);

        assert!(
            plan.operations
                .iter()
                .all(|operation| !operation.contains("Validation:")
                    && !operation.contains("cargo test")),
            "{:?}",
            plan.operations
        );
        assert_eq!(
            plan.validation_plan,
            vec!["Validation: cargo test -p design_cli --test integration"]
        );
    }

    #[test]
    fn instruction_plan_uses_sanitized_body_for_operations() {
        let input = "Target: apps/cli/src/repl.rs\n\
                     REPL long instruction flow の手動E2E確認用コメントを追加する。\n\
                     Risk: 不要な大規模変更をしない。\n\
                     cargo clippy -p design_cli --all-targets -- -D warnings";
        let plan = InstructionPlan::from_spec(input);
        let rendered_ops = plan.operations.join("\n");

        assert!(
            rendered_ops.contains("REPL long instruction flow"),
            "{rendered_ops}"
        );
        assert!(!rendered_ops.contains("Risk:"), "{rendered_ops}");
        assert!(!rendered_ops.contains("cargo clippy"), "{rendered_ops}");
    }

    #[test]
    fn instruction_plan_comment_instruction_renders_insert_comment() {
        let input = "Target: apps/cli/src/repl.rs\n\
                     REPL long instruction flow の手動E2E確認用コメントを追加する。";
        let plan = InstructionPlan::from_spec(input);

        assert!(
            plan.operations
                .iter()
                .any(|operation| operation.starts_with("InsertComment:")),
            "{:?}",
            plan.operations
        );
    }

    #[test]
    fn instruction_plan_warning_line_detected_as_risk() {
        let input = "Target: apps/cli/src/core.rs\nWarning: breaking change in API.";
        let plan = InstructionPlan::from_spec(input);

        assert_eq!(plan.risks.len(), 1, "{:?}", plan.risks);
        assert!(plan.risks[0].starts_with("Warning:"), "{:?}", plan.risks);
    }

    #[test]
    fn instruction_plan_cargo_clippy_detected_as_validation() {
        let input =
            "Target: apps/cli/src/core.rs\nRefactor.\ncargo clippy -p design_cli -- -D warnings";
        let plan = InstructionPlan::from_spec(input);

        assert_eq!(plan.validation_plan.len(), 1, "{:?}", plan.validation_plan);
        assert!(
            plan.validation_plan[0].contains("cargo clippy"),
            "{:?}",
            plan.validation_plan
        );
    }
}
