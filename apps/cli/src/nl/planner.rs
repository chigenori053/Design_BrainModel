use crate::nl::intent::{
    primary_intent, wants_analyze, wants_coding, wants_memory, wants_rules, wants_run,
    wants_structure_edit, wants_structure_view, wants_validate,
};
use crate::nl::target::resolve_target;
use crate::nl::types::{CodingOptions, CommandPlan, IntentType, PlannedStep};
use crate::plan::{CommandInvocation, Plan, PlanStatus, Step};
use crate::session::AgentSession;

pub fn plan_input(input: &str, session: &AgentSession) -> Option<CommandPlan> {
    let lower = input.to_lowercase();
    if lower.contains("whole project")
        || lower.contains("analyze project")
        || lower.contains("project .")
    {
        return None;
    }

    let primary = primary_intent(input);
    if primary == IntentType::Unknown {
        return None;
    }

    let target = resolve_target(input, session);
    let mut steps = Vec::new();

    if wants_structure_view(&lower) {
        steps.push(PlannedStep::StructureView(target.path));
        return Some(CommandPlan {
            intent: None,
            steps,
        });
    }

    if wants_structure_edit(&lower) {
        steps.push(PlannedStep::StructureEdit(target.path));
        return Some(CommandPlan {
            intent: None,
            steps,
        });
    }

    if wants_rules(&lower) {
        steps.push(PlannedStep::Rules);
        return Some(CommandPlan {
            intent: None,
            steps,
        });
    }

    if wants_memory(&lower) {
        steps.push(PlannedStep::Memory(target.path));
        return Some(CommandPlan {
            intent: None,
            steps,
        });
    }

    let analyze_first = wants_analyze(&lower)
        || (wants_coding(&lower)
            && ["unsafe", "循環", "cycle", "problem", "問題"]
                .iter()
                .any(|keyword| lower.contains(keyword)));

    if analyze_first {
        steps.push(PlannedStep::Analyze(target.path.clone()));
    }

    if wants_coding(&lower) {
        steps.push(PlannedStep::Coding(
            target.path.clone(),
            CodingOptions::default(),
        ));
    }

    if wants_validate(&lower) {
        steps.push(PlannedStep::Validate(target.path.clone()));
    }

    if wants_run(&lower) {
        steps.push(PlannedStep::Run(target.path.clone()));
    }

    if steps.is_empty() && wants_analyze(&lower) {
        steps.push(PlannedStep::Analyze(target.path));
    }

    if steps.is_empty() {
        None
    } else {
        Some(CommandPlan {
            intent: None,
            steps,
        })
    }
}

pub fn to_legacy_plan(command_plan: &CommandPlan) -> Plan {
    let mut steps = Vec::new();
    for (index, planned) in command_plan.steps.iter().enumerate() {
        let (description, command) = match planned {
            PlannedStep::Analyze(path) => (
                format!("Analyze: {}", path.display()),
                Some(CommandInvocation::new(
                    "analyze",
                    Some("code"),
                    &[&path.display().to_string()],
                )),
            ),
            PlannedStep::Coding(path, options) => {
                let mut args = vec![path.display().to_string()];
                if options.safe {
                    args.push("--safe".to_string());
                }
                if options.check {
                    args.push("--check".to_string());
                }
                (
                    format!("Coding: {}", path.display()),
                    Some(CommandInvocation {
                        name: "coding".to_string(),
                        subcommand: None,
                        args,
                    }),
                )
            }
            PlannedStep::Validate(path) => (
                format!("Validate: {}", path.display()),
                Some(CommandInvocation::new(
                    "validate",
                    None,
                    &[&path.display().to_string()],
                )),
            ),
            PlannedStep::StructureView(path) => (
                format!("Structure view: {}", path.display()),
                Some(CommandInvocation {
                    name: "structure".to_string(),
                    subcommand: Some("view".to_string()),
                    args: vec![path.display().to_string()],
                }),
            ),
            PlannedStep::StructureEdit(path) => (
                format!("Structure edit: {}", path.display()),
                Some(CommandInvocation {
                    name: "structure".to_string(),
                    subcommand: Some("edit".to_string()),
                    args: vec![path.display().to_string()],
                }),
            ),
            PlannedStep::StructureDiff(path, _) => (
                format!("Structure diff: {}", path.display()),
                Some(CommandInvocation {
                    name: "structure".to_string(),
                    subcommand: Some("dispatch".to_string()),
                    args: vec![path.display().to_string()],
                }),
            ),
            PlannedStep::StructureUndo(path) => (
                format!("Structure undo: {}", path.display()),
                Some(CommandInvocation {
                    name: "structure".to_string(),
                    subcommand: Some("undo".to_string()),
                    args: vec![path.display().to_string()],
                }),
            ),
            PlannedStep::StructureRedo(path) => (
                format!("Structure redo: {}", path.display()),
                Some(CommandInvocation {
                    name: "structure".to_string(),
                    subcommand: Some("redo".to_string()),
                    args: vec![path.display().to_string()],
                }),
            ),
            PlannedStep::Run(path) => (
                format!("Run: {}", path.display()),
                Some(CommandInvocation {
                    name: "run".to_string(),
                    subcommand: None,
                    args: vec![path.display().to_string()],
                }),
            ),
            PlannedStep::Rules => (
                "Rules list".to_string(),
                Some(CommandInvocation {
                    name: "rules".to_string(),
                    subcommand: Some("list".to_string()),
                    args: Vec::new(),
                }),
            ),
            PlannedStep::Memory(path) => (
                format!("Memory import: {}", path.display()),
                Some(CommandInvocation {
                    name: "memory".to_string(),
                    subcommand: Some("import".to_string()),
                    args: vec![path.display().to_string()],
                }),
            ),
            PlannedStep::GitCommit(path) => (
                format!("Git commit: {}", path.display()),
                Some(CommandInvocation {
                    name: "execute".to_string(),
                    subcommand: None,
                    args: vec![
                        "commit changes".to_string(),
                        "--path".to_string(),
                        path.display().to_string(),
                        "--dry-run".to_string(),
                    ],
                }),
            ),
            PlannedStep::GitPR(path) => (
                format!("Git PR: {}", path.display()),
                Some(CommandInvocation {
                    name: "execute".to_string(),
                    subcommand: None,
                    args: vec![
                        "push and create pr".to_string(),
                        "--path".to_string(),
                        path.display().to_string(),
                        "--dry-run".to_string(),
                        "--auto-remote".to_string(),
                    ],
                }),
            ),
            PlannedStep::AlternativeMutationSearch(spec) => (
                format!("Alternative mutation search: {spec}"),
                Some(CommandInvocation::new("analyze", Some("project"), &["."])),
            ),
            PlannedStep::DesignDeltaReasoning(spec) => (
                format!("Design delta reasoning: {spec}"),
                Some(CommandInvocation::new("analyze", Some("project"), &["."])),
            ),
            PlannedStep::ExplainDesignTradeoff(prompt) => (
                format!("Explain design tradeoff: {prompt}"),
                Some(CommandInvocation::new("analyze", Some("project"), &["."])),
            ),
            PlannedStep::ApplyPreviousCodingStep => (
                "Apply previous coding transaction".to_string(),
                Some(CommandInvocation {
                    name: "coding".to_string(),
                    subcommand: None,
                    args: vec!["--apply".to_string()],
                }),
            ),
            PlannedStep::RollbackCurrentTransaction => (
                "Rollback current IR transaction".to_string(),
                Some(CommandInvocation {
                    name: "rollback".to_string(),
                    subcommand: None,
                    args: Vec::new(),
                }),
            ),
        };
        steps.push(Step::new(index, description, command));
    }

    let mut plan = Plan::new("nl-plan", steps);
    plan.status = PlanStatus::Ready;
    plan
}

pub fn to_runtime_plan(command_plan: &CommandPlan) -> Plan {
    to_legacy_plan(command_plan)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn single_intent_analyze_maps_to_analyze_step() {
        let session = AgentSession::new();
        let plan = plan_input("このプロジェクトを解析して", &session).expect("plan");
        assert_eq!(plan.steps, vec![PlannedStep::Analyze(PathBuf::from("."))]);
    }

    #[test]
    fn coding_defaults_to_safe_and_check() {
        let session = AgentSession::new();
        let plan = plan_input("安全に修正して", &session).expect("plan");
        assert_eq!(
            plan.steps,
            vec![PlannedStep::Coding(
                PathBuf::from("."),
                CodingOptions {
                    safe: true,
                    check: true,
                    request: None,
                }
            )]
        );
    }

    #[test]
    fn composite_plan_analyze_then_coding_then_validate() {
        let session = AgentSession::new();
        let plan = plan_input("unsafeを減らして cargo check して", &session).expect("plan");
        assert_eq!(
            plan.steps,
            vec![
                PlannedStep::Analyze(PathBuf::from(".")),
                PlannedStep::Coding(PathBuf::from("."), CodingOptions::default()),
                PlannedStep::Validate(PathBuf::from(".")),
            ]
        );
    }
}
