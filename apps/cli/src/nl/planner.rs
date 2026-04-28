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
