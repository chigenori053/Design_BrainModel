use std::path::PathBuf;

use serde_json::Value;

use crate::design_delta::{self, bridge::to_command_plan, explain};
use crate::nl::session::{ConversationState, LastCodingTransaction};
use crate::nl::types::{CodingOptions, CommandPlan, PlannedStep};
use crate::nl_executor::run_design_command;
use crate::refactor::{GuiAction, GuiActionMode};
use crate::session::AgentSession;
use crate::state::State;

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

pub fn execute_plan(
    plan: &CommandPlan,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
) -> Vec<String> {
    let mut outputs = Vec::new();

    for (index, step) in plan.steps.iter().enumerate() {
        if cfg!(test)
            && let Some(snapshot) = executor_received_path_snapshot(step)
        {
            outputs.push(format!("[step {index}] {snapshot}"));
        }

        // R2: ApplyPreviousCodingStep は generic executor を bypass して直接処理する。
        if matches!(step, PlannedStep::ApplyPreviousCodingStep) {
            let output = execute_apply_previous_coding_step(conversation);
            outputs.push(format!("[step {index}] apply_previous_coding\n{output}"));
            continue;
        }

        if let PlannedStep::DesignDeltaReasoning(spec) = step {
            let output = execute_design_delta_reasoning_step(spec, session, conversation);
            outputs.push(format!("[step {index}] design_delta_reasoning\n{output}"));
            continue;
        }
        if let PlannedStep::AlternativeMutationSearch(spec) = step {
            let output = execute_alternative_mutation_search_step(spec, session, conversation);
            outputs.push(format!("[step {index}] alternative_mutation_search\n{output}"));
            continue;
        }
        if let PlannedStep::ExplainDesignTradeoff(prompt) = step {
            let output = execute_design_tradeoff_explanation_step(prompt, session, conversation);
            outputs.push(format!("[step {index}] explain_design_tradeoff\n{output}"));
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

pub fn executor_received_path_snapshot(step: &PlannedStep) -> Option<String> {
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
        | PlannedStep::GitPR(path) => Some(format!(
            "snapshot executor received path: {}",
            path.display()
        )),
        PlannedStep::StructureDiff(path, _) => Some(format!(
            "snapshot executor received path: {}",
            path.display()
        )),
        PlannedStep::Rules
        | PlannedStep::ApplyPreviousCodingStep
        | PlannedStep::DesignDeltaReasoning(_)
        | PlannedStep::AlternativeMutationSearch(_)
        | PlannedStep::ExplainDesignTradeoff(_) => None,
    }
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
    let is_file_target =
        path_str.ends_with(".rs") || path_str.ends_with(".toml") || path_str.ends_with(".md");
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
                    format!(
                        "design_cli coding . --target {} --safe --check",
                        path.display()
                    )
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
        PlannedStep::DesignDeltaReasoning(spec) => (
            "design-delta",
            vec![spec.clone()],
            "design delta reasoning loop".to_string(),
            true,
        ),
        PlannedStep::AlternativeMutationSearch(spec) => (
            "design-delta-search",
            vec![spec.clone()],
            "alternative mutation search loop".to_string(),
            true,
        ),
        PlannedStep::ExplainDesignTradeoff(prompt) => (
            "design-delta-explain",
            vec![prompt.clone()],
            "design tradeoff explanation loop".to_string(),
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
        PlannedStep::DesignDeltaReasoning(_)
        | PlannedStep::AlternativeMutationSearch(_)
        | PlannedStep::ExplainDesignTradeoff(_) => {}
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

fn execute_design_delta_reasoning_step(
    spec: &str,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
) -> String {
    let root = session
        .workspace_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));

    session.state = State::SpecReceived;
    let output = match design_delta::run_reasoning_loop(&root, spec) {
        Ok(output) => output,
        Err(err) => return format!("Accepted: false\nReason: {err}"),
    };

    session.state = State::DesignDeltaReady;
    session.design_baseline = Some(output.baseline.clone());
    conversation.last_design_delta = Some(output.delta.clone());

    session.state = State::MutationPlanned;
    session.active_mutation_plan = Some(output.mutation_plan.clone());
    conversation.active_mutation_plan = Some(output.mutation_plan.clone());

    session.state = State::RationalityScored;
    session.last_rationality_score = Some(output.rationality.clone());
    conversation.last_rationality_score = Some(output.rationality.clone());

    if !output.accepted {
        session.state = State::Error;
        return format!(
            "Accepted: false\nRationality total: {:.2}\nFallback retries: {}\nReason: below threshold",
            output.rationality.total, output.retries
        );
    }

    session.state = State::PatchPlanReady;
    conversation.last_patch_plan = Some(output.patch_plan.clone());
    conversation.last_analysis_summary = Some(output.patch_plan.summary.clone());

    let mut lines = vec![
        "Accepted: true".to_string(),
        format!("Impacted crates: {}", output.delta.impacted_crates.join(", ")),
        format!("Rationality total: {:.2}", output.rationality.total),
        format!(
            "Expected tests: {}",
            output.patch_plan.expected_tests.join(" | ")
        ),
    ];

    session.state = State::TestPlanReady;
    let follow_up = to_command_plan(&output.mutation_plan, spec);
    let follow_up_outputs = execute_plan(&follow_up, session, conversation);
    if follow_up_outputs
        .iter()
        .any(|line| line.starts_with("[step") && line.contains("Error:"))
    {
        session.state = State::Repairing;
    } else {
        session.state = State::CommitReady;
    }
    lines.extend(follow_up_outputs);
    lines.join("\n")
}

fn execute_alternative_mutation_search_step(
    spec: &str,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
) -> String {
    let root = session
        .workspace_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));

    session.state = State::SpecReceived;
    let output = match design_delta::run_alternative_search_loop(&root, spec) {
        Ok(output) => output,
        Err(err) => return format!("Accepted: false\nReason: {err}"),
    };

    session.state = State::MutationCandidatesReady;
    if let Some(search_result) = &output.search_result {
        session.mutation_candidates = search_result.candidates.clone();
        conversation.mutation_candidates = search_result.candidates.clone();
        session.mutation_search_depth = 3;
        conversation.mutation_search_depth = 3;
        session.last_mutation_search_result = Some(search_result.clone());
        conversation.last_mutation_search_result = Some(search_result.clone());
    }

    session.state = State::MutationRankingReady;
    if let Some(search_result) = &output.search_result
        && let Some(selected) = search_result.selected.clone()
    {
        session.selected_mutation = Some(selected.clone());
        conversation.selected_mutation = Some(selected);
    }

    session.state = State::BestMutationSelected;
    session.design_baseline = Some(output.baseline.clone());
    conversation.last_design_delta = Some(output.delta.clone());
    session.active_mutation_plan = Some(output.mutation_plan.clone());
    conversation.active_mutation_plan = Some(output.mutation_plan.clone());
    session.last_rationality_score = Some(output.rationality.clone());
    conversation.last_rationality_score = Some(output.rationality.clone());

    if !output.accepted {
        session.state = State::Error;
        return format!(
            "Accepted: false\nCandidates: {}\nRationality total: {:.2}\nReason: below search threshold",
            output
                .search_result
                .as_ref()
                .map(|r| r.candidates.len())
                .unwrap_or(0),
            output.rationality.total
        );
    }

    session.state = State::PatchPlanReady;
    conversation.last_patch_plan = Some(output.patch_plan.clone());
    conversation.last_analysis_summary = Some(output.patch_plan.summary.clone());

    let selected_id = session
        .selected_mutation
        .as_ref()
        .map(|candidate| candidate.id.clone())
        .unwrap_or_else(|| "unknown".to_string());
    let mut lines = vec![
        format!(
            "Candidates: {}",
            output
                .search_result
                .as_ref()
                .map(|r| r.candidates.len())
                .unwrap_or(0)
        ),
        format!("Selected mutation: {selected_id}"),
        format!("Rationality total: {:.2}", output.rationality.total),
    ];

    session.state = State::TestPlanReady;
    let follow_up = to_command_plan(&output.mutation_plan, spec);
    let follow_up_outputs = execute_plan(&follow_up, session, conversation);
    if follow_up_outputs
        .iter()
        .any(|line| line.starts_with("[step") && line.contains("Error:"))
    {
        session.state = State::Repairing;
    } else {
        session.state = State::CommitReady;
    }
    lines.extend(follow_up_outputs);
    lines.join("\n")
}

fn execute_design_tradeoff_explanation_step(
    _prompt: &str,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
) -> String {
    let search_result = conversation
        .last_mutation_search_result
        .clone()
        .or_else(|| session.last_mutation_search_result.clone());
    let Some(search_result) = search_result else {
        return "No tradeoff explanation available. Run alternative mutation search first.".to_string();
    };

    let Some(explanation) = explain::explain_tradeoff(&search_result) else {
        return "No tradeoff explanation available. Selected and rejected mutations were not retained.".to_string();
    };
    session.last_tradeoff_explanation = Some(explanation.clone());
    conversation.last_tradeoff_explanation = Some(explanation.clone());
    explain::render_pr_ready_block(&explanation)
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
    use crate::design_delta::{
        DesignDelta, MutationCandidate, MutationPlan, MutationSearchResult, MutationStrategy,
        RationalityScore,
    };
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

    #[test]
    fn executor_snapshot_preserves_exact_coding_rs_path() {
        let snapshot = executor_received_path_snapshot(&PlannedStep::Analyze(PathBuf::from(
            "apps/cli/src/coding.rs",
        )))
        .expect("snapshot");
        assert!(snapshot.ends_with("apps/cli/src/coding.rs"), "{snapshot}");
    }

    #[test]
    fn executor_snapshot_preserves_exact_runtime_vm_lib_path() {
        let snapshot = executor_received_path_snapshot(&PlannedStep::Analyze(PathBuf::from(
            "crates/runtime/runtime_vm/src/lib.rs",
        )))
        .expect("snapshot");
        assert!(
            snapshot.ends_with("crates/runtime/runtime_vm/src/lib.rs"),
            "{snapshot}"
        );
    }

    #[test]
    fn explain_tradeoff_step_emits_pr_ready_block() {
        let mut session = AgentSession::new();
        let mut conversation = ConversationState::default();
        let selected = MutationCandidate {
            id: "trait-extraction-1".to_string(),
            plan: MutationPlan {
                delta: DesignDelta {
                    impacted_crates: vec!["design_cli".to_string()],
                    ..DesignDelta::default()
                },
                target_files: vec![PathBuf::from("apps/cli/Cargo.toml")],
                expected_tests: vec!["cargo test -p design_cli".to_string()],
                rollback_units: vec!["crate::design_cli".to_string()],
            },
            expected_score: Some(RationalityScore {
                maintainability: 0.91,
                extensibility: 0.92,
                risk: 0.80,
                boundary_integrity: 0.93,
                rollback_complexity: 0.87,
                total: 0.89,
            }),
            strategy: MutationStrategy::TraitExtraction,
        };
        let rejected = MutationCandidate {
            id: "adapter-insertion-1".to_string(),
            plan: selected.plan.clone(),
            expected_score: Some(RationalityScore {
                maintainability: 0.84,
                extensibility: 0.80,
                risk: 0.82,
                boundary_integrity: 0.79,
                rollback_complexity: 0.86,
                total: 0.83,
            }),
            strategy: MutationStrategy::AdapterInsertion,
        };
        conversation.last_mutation_search_result = Some(MutationSearchResult {
            candidates: vec![selected.clone(), rejected.clone()],
            selected: Some(selected),
            rejected: vec![rejected.id.clone()],
        });

        let output =
            execute_design_tradeoff_explanation_step("設計トレードオフを要約して", &mut session, &mut conversation);
        assert!(output.contains("Selected mutation: trait-extraction-1"));
        assert!(output.contains("Tradeoff summary:"));
        assert!(output.contains("Rejected:"));
        assert!(conversation.last_tradeoff_explanation.is_some());
    }
}
