use std::path::PathBuf;

use serde_json::Value;

use crate::nl::session::ConversationState;
use crate::nl::types::{CommandPlan, PlannedStep};
use crate::nl_executor::run_design_command;
use crate::refactor::{GuiAction, GuiActionMode};

pub fn render_plan_summary(plan: &CommandPlan) -> String {
    format!("[planner: nl] {} steps", plan.steps.len())
}

pub fn describe_plan_labels(plan: &CommandPlan) -> Vec<String> {
    plan.steps
        .iter()
        .map(|step| to_canonical_command(step).2)
        .collect()
}

pub fn execute_plan(plan: &CommandPlan, conversation: &mut ConversationState) -> Vec<String> {
    let mut outputs = Vec::new();

    for (index, step) in plan.steps.iter().enumerate() {
        if let Err(err) = ensure_viewer_session_is_fresh(step, conversation) {
            outputs.push(format!("[step {index}] Error: {err}"));
            break;
        }
        let (command, args, label, skip_exec) = to_canonical_command(step);
        let result = if skip_exec {
            Ok(String::new())
        } else {
            run_design_command(command, &args)
        };
        match result {
            Ok(output) => {
                update_state_after_step(step, conversation);
                outputs.push(format!("[step {index}] {label}\n{output}"));
            }
            Err(err) => {
                outputs.push(format!("[step {index}] Error: {err}"));
                break;
            }
        }
    }

    outputs
}

fn to_canonical_command(step: &PlannedStep) -> (&'static str, Vec<String>, String, bool) {
    match step {
        PlannedStep::Analyze(path) => (
            "analyze",
            vec![path.display().to_string()],
            format!("design_cli analyze {}", path.display()),
            false,
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
                "coding",
                args,
                format!("design_cli coding {} --safe --check", path.display()),
                false,
            )
        }
        PlannedStep::Validate(path) => (
            "validate",
            vec![path.display().to_string()],
            format!("design_cli validate {}", path.display()),
            false,
        ),
        PlannedStep::StructureView(path) => (
            "structure",
            vec!["view".to_string(), path.display().to_string(), "--json".to_string()],
            format!("design_cli structure view {}", path.display()),
            false,
        ),
        PlannedStep::StructureEdit(path) => (
            "structure",
            vec!["edit".to_string(), path.display().to_string(), "--json".to_string()],
            format!("design_cli structure edit {}", path.display()),
            false,
        ),
        PlannedStep::StructureDiff(path, node) => (
            "structure",
            build_structure_diff_args(path, node.as_deref()),
            format!("design_cli structure dispatch {} --event <generated diff>", path.display()),
            false,
        ),
        PlannedStep::StructureUndo(path) => (
            "structure",
            vec!["undo".to_string(), path.display().to_string(), "--json".to_string()],
            format!("design_cli structure undo {}", path.display()),
            false,
        ),
        PlannedStep::StructureRedo(path) => (
            "structure",
            vec!["redo".to_string(), path.display().to_string(), "--json".to_string()],
            format!("design_cli structure redo {}", path.display()),
            false,
        ),
        PlannedStep::Run(path) => (
            "run",
            vec![path.display().to_string()],
            format!("design_cli run {}", path.display()),
            false,
        ),
        PlannedStep::Rules => (
            "rules",
            vec!["list".to_string()],
            "design_cli rules list".to_string(),
            false,
        ),
        PlannedStep::Memory(path) => (
            "memory",
            vec!["import".to_string(), path.display().to_string()],
            format!("design_cli memory import {}", path.display()),
            false,
        ),
        PlannedStep::GitCommit(path) => (
            "execute",
            vec![
                "commit changes".to_string(),
                "--path".to_string(),
                path.display().to_string(),
                "--dry-run".to_string(),
                "--json".to_string(),
            ],
            format!(
                "design_cli execute \"commit changes\" --path {} --dry-run --json [confirmation required, branch != main]",
                path.display()
            ),
            true,
        ),
        PlannedStep::GitPR(path) => (
            "execute",
            vec![
                "push and create pr".to_string(),
                "--path".to_string(),
                path.display().to_string(),
                "--dry-run".to_string(),
                "--auto-remote".to_string(),
                "--json".to_string(),
            ],
            format!(
                "design_cli execute \"push and create pr\" --path {} --dry-run --auto-remote --json [confirmation required, branch != main]",
                path.display()
            ),
            true,
        ),
    }
}

fn build_structure_diff_args(path: &PathBuf, node: Option<&str>) -> Vec<String> {
    let event = GuiAction {
        action: "refactor".to_string(),
        target: "auto".to_string(),
        node: node.map(|node| node.to_string()),
        project_root: None,
        selected_nodes: Vec::new(),
        selected_edges: Vec::new(),
        mode: GuiActionMode::Preview,
    };
    let event_path = std::env::temp_dir().join(format!(
        "design_cli_structure_diff_{}.json",
        uuid::Uuid::new_v4()
    ));
    if let Ok(serialized) = serde_json::to_string(&event) {
        let _ = std::fs::write(&event_path, serialized);
    }
    vec![
        "dispatch".to_string(),
        path.display().to_string(),
        "--event".to_string(),
        event_path.display().to_string(),
        "--json".to_string(),
    ]
}

fn update_state_after_step(step: &PlannedStep, conversation: &mut ConversationState) {
    match step {
        PlannedStep::Analyze(path)
        | PlannedStep::Coding(path, _)
        | PlannedStep::Validate(path)
        | PlannedStep::StructureView(path)
        | PlannedStep::StructureEdit(path)
        | PlannedStep::StructureUndo(path)
        | PlannedStep::StructureRedo(path)
        | PlannedStep::Run(path)
        | PlannedStep::Memory(path)
        | PlannedStep::GitCommit(path)
        | PlannedStep::GitPR(path) => conversation.last_target = Some(path.clone()),
        PlannedStep::StructureDiff(path, node) => {
            conversation.last_target = Some(path.clone());
            if let Some(node) = node {
                conversation.last_node = Some(node.clone());
            }
        }
        PlannedStep::Rules => {}
    }

    if matches!(
        step,
        PlannedStep::StructureView(_)
            | PlannedStep::StructureEdit(_)
            | PlannedStep::StructureDiff(_, _)
            | PlannedStep::StructureUndo(_)
            | PlannedStep::StructureRedo(_)
    ) && let Some(path) = &conversation.last_target
        && let Ok(raw) = run_design_command(
            "structure",
            &[
                "session".to_string(),
                path.display().to_string(),
                "--json".to_string(),
            ],
        )
        && let Ok(json) = serde_json::from_str::<Value>(&raw)
        && let Some(session_id) = json.get("session_id").and_then(Value::as_str)
    {
        conversation.last_viewer_session = Some(session_id.to_string());
    }
}

fn ensure_viewer_session_is_fresh(
    step: &PlannedStep,
    conversation: &ConversationState,
) -> Result<(), String> {
    let path = match step {
        PlannedStep::StructureDiff(path, _)
        | PlannedStep::StructureUndo(path)
        | PlannedStep::StructureRedo(path) => path,
        _ => return Ok(()),
    };

    let Some(expected) = conversation.last_viewer_session.as_deref() else {
        return Ok(());
    };

    let raw = run_design_command(
        "structure",
        &[
            "session".to_string(),
            path.display().to_string(),
            "--json".to_string(),
        ],
    )?;
    let json: Value = serde_json::from_str(&raw).map_err(|err| err.to_string())?;
    let current = json.get("session_id").and_then(Value::as_str).unwrap_or_default();
    if current == expected {
        Ok(())
    } else {
        Err(format!(
            "stale viewer session: expected {expected}, current {current}"
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::nl::types::{CodingOptions, PlannedStep};

    #[test]
    fn summary_uses_nl_label() {
        let plan = CommandPlan {
            steps: vec![PlannedStep::Analyze(PathBuf::from("."))],
        };
        assert_eq!(render_plan_summary(&plan), "[planner: nl] 1 steps");
    }

    #[test]
    fn coding_step_maps_to_safe_command() {
        let (_, args, label, _) = to_canonical_command(&PlannedStep::Coding(
            PathBuf::from("."),
            CodingOptions::default(),
        ));
        assert_eq!(args, vec![".".to_string(), "--safe".to_string(), "--check".to_string()]);
        assert!(label.contains("--safe --check"));
    }
}
