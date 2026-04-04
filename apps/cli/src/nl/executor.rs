use std::path::PathBuf;

use serde_json::Value;

use crate::nl::session::{ConversationState, LastCodingTransaction};
use crate::nl::types::{CodingOptions, CommandPlan, PlannedStep};
use crate::nl_executor::run_design_command;
use crate::refactor::{GuiAction, GuiActionMode};

pub fn render_plan_summary(plan: &CommandPlan) -> String {
    render_plan_summary_with_label(plan, "nl_v2")
}

pub fn render_plan_summary_with_label(plan: &CommandPlan, label: &str) -> String {
    format!("[planner: {label}] {} steps", plan.steps.len())
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
        // R2: ApplyPreviousCodingStep は generic executor を bypass して直接処理する。
        if matches!(step, PlannedStep::ApplyPreviousCodingStep) {
            let output = execute_apply_previous_coding_step(conversation);
            outputs.push(format!("[step {index}] apply_previous_coding\n{output}"));
            continue;
        }

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
                // Coding step の dry-run 完了後、transaction を記録する (R3)。
                if let PlannedStep::Coding(path, opts) = step {
                    if opts.check {
                        record_coding_transaction(path, opts, &output, conversation);
                    }
                }
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

/// R4: 前回 checked coding transaction を --apply へ昇格して実行する。
///
/// R5: patch_count == 0 → no-op
/// R6: applied == true → no-op
fn execute_apply_previous_coding_step(conversation: &mut ConversationState) -> String {
    let tx = match conversation.last_coding_transaction.as_ref() {
        Some(tx) => tx.clone(),
        None => return "Applied: false\nReason: no previous coding transaction".to_string(),
    };

    if tx.applied {
        return "Applied: false\nReason: already applied".to_string();
    }

    if tx.patch_count == 0 {
        return "Applied: false\nReason: no pending canonical patches".to_string();
    }

    // R4: --check → --apply の deterministic 変換。target / request は R3 で保持済み。
    let path_str = tx.target.display().to_string();
    let is_file_target = path_str.ends_with(".rs")
        || path_str.ends_with(".toml")
        || path_str.ends_with(".md");
    let mut args = if is_file_target {
        vec![".".to_string(), "--target".to_string(), path_str]
    } else {
        vec![path_str]
    };
    if let Some(request) = &tx.request {
        args.push("--request".to_string());
        args.push(request.clone());
    }
    if tx.safe {
        args.push("--safe".to_string());
    }
    args.push("--apply".to_string());

    match run_design_command("coding", &args) {
        Ok(output) => {
            // R6: apply 済みとしてマークし、重複 apply を防止する。
            if let Some(tx) = conversation.last_coding_transaction.as_mut() {
                tx.applied = true;
            }
            // R3: target continuity を維持する。
            conversation.last_target = Some(tx.target.clone());
            format!("Applied: true\n{output}")
        }
        Err(err) => format!("Applied: false\nReason: {err}"),
    }
}

/// Coding dry-run 後に `LastCodingTransaction` を conversation に記録する (R3)。
///
/// patch_count は JSON 出力から `patches` 配列長で取得する。
/// パース失敗時は 1 (non-zero) にフォールバックして apply を許可する。
fn record_coding_transaction(
    path: &PathBuf,
    opts: &CodingOptions,
    output: &str,
    conversation: &mut ConversationState,
) {
    let patch_count = serde_json::from_str::<Value>(output)
        .ok()
        .and_then(|v| {
            // CodingReport.patches または changes.patches を優先的に参照する。
            v.get("patches")
                .and_then(|p| p.as_array())
                .map(|a| a.len())
                .or_else(|| {
                    v.get("changes")
                        .and_then(|c| c.get("patches"))
                        .and_then(|p| p.as_array())
                        .map(|a| a.len())
                })
        })
        .unwrap_or(1); // パース不能時は non-zero にして apply を許可する

    conversation.last_coding_transaction = Some(LastCodingTransaction {
        target: path.clone(),
        request: opts.request.clone(),
        safe: opts.safe,
        patch_count,
        applied: false,
    });
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
            let path_str = path.display().to_string();
            let is_file_target = path_str.ends_with(".rs")
                || path_str.ends_with(".toml")
                || path_str.ends_with(".md");
            let mut args = if is_file_target {
                vec![".".to_string(), "--target".to_string(), path_str.clone()]
            } else {
                vec![path_str.clone()]
            };
            if let Some(request) = &options.request {
                args.push("--request".to_string());
                args.push(request.clone());
            }
            if options.safe {
                args.push("--safe".to_string());
            }
            if options.check {
                args.push("--check".to_string());
            }
            (
                "coding",
                args,
                if is_file_target {
                    format!("design_cli coding . --target {} --safe --check", path.display())
                } else {
                    format!("design_cli coding {} --safe --check", path.display())
                },
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
            vec![
                "view".to_string(),
                path.display().to_string(),
                "--json".to_string(),
            ],
            format!("design_cli structure view {}", path.display()),
            false,
        ),
        PlannedStep::StructureEdit(path) => (
            "structure",
            vec![
                "edit".to_string(),
                path.display().to_string(),
                "--json".to_string(),
            ],
            format!("design_cli structure edit {}", path.display()),
            false,
        ),
        PlannedStep::StructureDiff(path, node) => (
            "structure",
            build_structure_diff_args(path, node.as_deref()),
            format!(
                "design_cli structure dispatch {} --event <generated diff>",
                path.display()
            ),
            false,
        ),
        PlannedStep::StructureUndo(path) => (
            "structure",
            vec![
                "undo".to_string(),
                path.display().to_string(),
                "--json".to_string(),
            ],
            format!("design_cli structure undo {}", path.display()),
            false,
        ),
        PlannedStep::StructureRedo(path) => (
            "structure",
            vec![
                "redo".to_string(),
                path.display().to_string(),
                "--json".to_string(),
            ],
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
        // ApplyPreviousCodingStep は execute_plan で直接処理されるため、
        // to_canonical_command には到達しない。describe_plan_labels 用の記述のみ。
        PlannedStep::ApplyPreviousCodingStep => (
            "coding",
            vec!["--apply".to_string()],
            "design_cli coding --apply [previous transaction]".to_string(),
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
        // ApplyPreviousCodingStep は execute_plan 内で直接処理されるため、
        // update_state_after_step には到達しない。last_target は apply 実行時に更新済み。
        PlannedStep::ApplyPreviousCodingStep => {}
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
    let current = json
        .get("session_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
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
        assert_eq!(render_plan_summary(&plan), "[planner: nl_v2] 1 steps");
    }

    #[test]
    fn coding_step_maps_to_safe_command() {
        let (_, args, label, _) = to_canonical_command(&PlannedStep::Coding(
            PathBuf::from("."),
            CodingOptions::default(),
        ));
        assert_eq!(
            args,
            vec![".".to_string(), "--safe".to_string(), "--check".to_string()]
        );
        assert!(label.contains("--safe --check"));
    }

    #[test]
    fn coding_step_with_file_target_remaps_to_target_flag() {
        let (_, args, label, _) = to_canonical_command(&PlannedStep::Coding(
            PathBuf::from("src/coding.rs"),
            CodingOptions::default(),
        ));
        assert_eq!(
            args,
            vec![
                ".".to_string(),
                "--target".to_string(),
                "src/coding.rs".to_string(),
                "--safe".to_string(),
                "--check".to_string(),
            ]
        );
        assert!(label.contains("design_cli coding . --target src/coding.rs --safe --check"));
    }

    #[test]
    fn coding_step_passes_request_through_hidden_flag() {
        let (_, args, _, _) = to_canonical_command(&PlannedStep::Coding(
            PathBuf::from("."),
            CodingOptions {
                request: Some("repl.rs の planner_v2 接続を修正して".to_string()),
                ..CodingOptions::default()
            },
        ));
        assert_eq!(args[0], ".");
        assert_eq!(args[1], "--request");
        assert_eq!(args[2], "repl.rs の planner_v2 接続を修正して");
    }
}
