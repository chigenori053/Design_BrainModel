use std::path::PathBuf;

use serde_json::Value;

use crate::coding::{
    apply_code_change_set, build_unified_diff_preview, generate_code_change_set,
    render_code_diff_lines,
};
use crate::design_delta::{self, explain};
use crate::ir::{IRPersistenceArtifact, persist_ir_transition};
use crate::nl::session::ConversationState;
use crate::nl::types::{CodingOptions, CommandPlan, PlannedStep};
use crate::nl_executor::run_design_command;
use crate::refactor::{GuiAction, GuiActionMode, RefactorTarget, create_refactor_plan};
use crate::service::{
    AnalysisDependency, AnalysisModule, analyze_path,
    dto::{ActionKind, SessionAppliedDiff, SessionAppliedFileDiff},
};
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
        if matches!(step, PlannedStep::RollbackCurrentTransaction) {
            let output = execute_ir_rollback(conversation);
            outputs.push(format!(
                "[step {index}] rollback_current_transaction\n{output}"
            ));
            continue;
        }

        if let PlannedStep::DesignDeltaReasoning(spec) = step {
            let output = execute_design_delta_reasoning_step(spec, session, conversation);
            outputs.push(format!("[step {index}] design_delta_reasoning\n{output}"));
            continue;
        }
        if let PlannedStep::AlternativeMutationSearch(spec) = step {
            let output = execute_alternative_mutation_search_step(spec, session, conversation);
            outputs.push(format!(
                "[step {index}] alternative_mutation_search\n{output}"
            ));
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
        | PlannedStep::RollbackCurrentTransaction
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
    let tx = match conversation.active_transaction() {
        Some(tx) => tx.clone(),
        None => return "Applied: false\nReason: no previous coding transaction".to_string(),
    };

    if tx.applied && !tx.pending {
        return "Applied: false\nReason: already applied".to_string();
    }

    let Some((path, request, safe)) = last_preview_coding_context(conversation) else {
        return "Applied: false\nReason: missing preview context".to_string();
    };

    // R4: --check → --apply の deterministic 変換。canonical path / request は last_plan から再構築する。
    let path_str = path.display().to_string();
    let is_file_target =
        path_str.ends_with(".rs") || path_str.ends_with(".toml") || path_str.ends_with(".md");
    let mut args = if is_file_target {
        vec![".".to_string(), "--target".to_string(), path_str]
    } else {
        vec![path_str]
    };
    if let Some(request) = request {
        args.push("--request".to_string());
        args.push(request);
    }
    if safe {
        args.push("--safe".to_string());
    }
    args.push("--apply".to_string());

    match run_design_command("coding", &args) {
        Ok(output) => {
            let before = conversation.ir_state.clone();
            conversation.mark_transaction_applied(None);
            conversation.note_target(tx.canonical_target.clone());
            let _ = persist_ir_transition(
                &before,
                &conversation.ir_state,
                crate::service::dto::ActionKind::Apply,
                "apply_previous_coding",
                IRPersistenceArtifact::default(),
            );
            format!("Applied: true\n{output}")
        }
        Err(err) => format!("Applied: false\nReason: {err}"),
    }
}

fn last_preview_coding_context(
    conversation: &ConversationState,
) -> Option<(PathBuf, Option<String>, bool)> {
    let step = conversation
        .last_plan
        .as_ref()?
        .steps
        .iter()
        .rev()
        .find_map(|step| {
            if let PlannedStep::Coding(path, opts) = step {
                Some((path.clone(), opts.clone()))
            } else {
                None
            }
        })?;
    Some((step.0, step.1.request, step.1.safe))
}

fn execute_ir_rollback(conversation: &mut ConversationState) -> String {
    let before = conversation.ir_state.clone();
    conversation.rollback_current_transaction();
    let _ = persist_ir_transition(
        &before,
        &conversation.ir_state,
        crate::service::dto::ActionKind::Rollback,
        "rollback_current_transaction",
        IRPersistenceArtifact::default(),
    );
    "[rollback] reverted current IR transaction".to_string()
}

fn record_coding_transaction(
    path: &PathBuf,
    opts: &CodingOptions,
    output: &str,
    conversation: &mut ConversationState,
) {
    let _ = serde_json::from_str::<Value>(output).ok();
    let _ = opts;
    let before = conversation.ir_state.clone();
    conversation.start_preview_transaction(path.clone());
    let _ = persist_ir_transition(
        &before,
        &conversation.ir_state,
        crate::service::dto::ActionKind::CodingPreview,
        format!("preview {}", path.display()),
        IRPersistenceArtifact::default(),
    );
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
        PlannedStep::RollbackCurrentTransaction => (
            "rollback",
            Vec::new(),
            "design_cli rollback [ir transaction]".to_string(),
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
        | PlannedStep::ExplainDesignTradeoff(_)
        | PlannedStep::RollbackCurrentTransaction => {}
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
    log::debug!("Executing DesignDeltaReasoning");
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
        format!(
            "Impacted crates: {}",
            output.delta.impacted_crates.join(", ")
        ),
        format!("Rationality total: {:.2}", output.rationality.total),
        format!(
            "Expected tests: {}",
            output.patch_plan.expected_tests.join(" | ")
        ),
    ];

    session.state = State::TestPlanReady;
    let execution_output = execute_design_delta_refactor(
        spec,
        &output.delta,
        &output.mutation_plan,
        conversation,
    );
    if execution_output.contains("Error:") {
        session.state = State::Repairing;
    } else {
        session.state = State::CommitReady;
    }
    lines.push(execution_output);
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
    let execution_output = execute_design_delta_refactor(
        spec,
        &output.delta,
        &output.mutation_plan,
        conversation,
    );
    if execution_output.contains("Error:") {
        session.state = State::Repairing;
    } else {
        session.state = State::CommitReady;
    }
    lines.push(execution_output);
    lines.join("\n")
}

fn execute_design_delta_refactor(
    spec: &str,
    delta: &design_delta::DesignDelta,
    _mutation_plan: &design_delta::MutationPlan,
    conversation: &mut ConversationState,
) -> String {
    let root = if delta.workspace_root.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        delta.workspace_root.clone()
    };
    let analysis = match analyze_path(&root) {
        Ok(report) => report,
        Err(err) => return format!("Error: failed to analyze design delta target: {err}"),
    };
    let target = select_design_delta_target(&analysis.modules, &analysis.dependencies, spec, delta);
    let plan = match create_refactor_plan(&analysis, target) {
        Ok(plan) => plan,
        Err(err) => return format!("Error: failed to create refactor plan: {err}"),
    };
    let change_set = match generate_code_change_set(&plan.root, &plan.patches) {
        Ok(change_set) => change_set,
        Err(err) => return format!("Error: failed to generate change_set: {err}"),
    };
    if change_set.changes.is_empty() {
        return "No changes detected.".to_string();
    }

    let diff_preview = match build_unified_diff_preview(&plan.root, &change_set) {
        Ok(preview) => preview,
        Err(err) => return format!("Error: failed to generate diff: {err}"),
    };
    let snapshot = build_session_diff_snapshot(&diff_preview);
    let diff_text = render_design_delta_diff(&diff_preview);
    let canonical_target = change_set
        .canonical_target
        .clone()
        .or_else(|| {
            change_set
                .changes
                .first()
                .map(|change| PathBuf::from(&change.file_path))
        })
        .unwrap_or_else(|| PathBuf::from("."));

    let before = conversation.ir_state.clone();
    conversation.start_preview_transaction(canonical_target.clone());
    match apply_code_change_set(&plan.root, &change_set) {
        Ok(()) => {
            conversation.note_target(canonical_target);
            conversation.mark_transaction_applied(Some(snapshot.clone()));
            let _ = persist_ir_transition(
                &before,
                &conversation.ir_state,
                ActionKind::Apply,
                "design_delta_reasoning",
                IRPersistenceArtifact {
                    diff_ref: Some(snapshot.clone()),
                    build_ok: None,
                    ..IRPersistenceArtifact::default()
                },
            );
            format!(
                "DIFF\n{diff_text}\nRESULT\nRefactoring applied successfully.\n{} files changed, +{} -{} lines.",
                snapshot.files_changed, snapshot.lines_added, snapshot.lines_removed
            )
        }
        Err(err) => format!("Error: failed to apply change_set: {err}"),
    }
}

fn select_design_delta_target(
    modules: &[AnalysisModule],
    dependencies: &[AnalysisDependency],
    spec: &str,
    delta: &design_delta::DesignDelta,
) -> RefactorTarget {
    let lower = spec.to_lowercase();
    let module_name = select_impacted_module(modules, delta)
        .or_else(|| modules.first().map(|module| module.name.clone()))
        .unwrap_or_else(|| "core".to_string());
    let edge = select_impacted_dependency(dependencies, delta)
        .or_else(|| {
            dependencies
                .first()
                .map(|dependency| (dependency.from.clone(), dependency.to.clone()))
        })
        .unwrap_or_else(|| (module_name.clone(), "ports".to_string()));

    if lower.contains("splitmodule")
        || lower.contains("module split")
        || lower.contains("split module")
        || lower.contains("分割")
    {
        return RefactorTarget::ModuleSplit(module_name);
    }
    if lower.contains("removedependency")
        || lower.contains("remove dependency")
        || lower.contains("依存")
    {
        return RefactorTarget::RemoveDependency {
            from: edge.0,
            to: edge.1,
        };
    }
    if lower.contains("extractinterface")
        || lower.contains("extract interface")
        || lower.contains("trait")
        || lower.contains("interface")
        || lower.contains("抽象")
    {
        return RefactorTarget::ExtractInterface {
            from: edge.0,
            to: edge.1,
        };
    }
    if !delta.introduced_interfaces.is_empty() {
        return RefactorTarget::ExtractInterface {
            from: edge.0,
            to: edge.1,
        };
    }
    if !delta.dependency_moves.is_empty() {
        return RefactorTarget::RemoveDependency {
            from: edge.0,
            to: edge.1,
        };
    }
    RefactorTarget::ModuleSplit(module_name)
}

fn select_impacted_module(
    modules: &[AnalysisModule],
    delta: &design_delta::DesignDelta,
) -> Option<String> {
    modules
        .iter()
        .find(|module| {
            delta.impacted_crates.iter().any(|crate_name| {
                module.name.eq_ignore_ascii_case(crate_name)
                    || module
                        .source_path
                        .to_lowercase()
                        .contains(&crate_name.to_lowercase())
            })
        })
        .map(|module| module.name.clone())
}

fn select_impacted_dependency(
    dependencies: &[AnalysisDependency],
    delta: &design_delta::DesignDelta,
) -> Option<(String, String)> {
    dependencies
        .iter()
        .find(|dependency| {
            delta.impacted_crates.iter().any(|crate_name| {
                dependency.from.eq_ignore_ascii_case(crate_name)
                    || dependency.to.eq_ignore_ascii_case(crate_name)
                    || dependency
                        .from
                        .to_lowercase()
                        .contains(&crate_name.to_lowercase())
                    || dependency
                        .to
                        .to_lowercase()
                        .contains(&crate_name.to_lowercase())
            })
        })
        .map(|dependency| (dependency.from.clone(), dependency.to.clone()))
}

fn build_session_diff_snapshot(
    diff_preview: &crate::coding::UnifiedDiffPreview,
) -> SessionAppliedDiff {
    SessionAppliedDiff {
        summary: format!(
            "{} files changed, +{} -{} lines",
            diff_preview.summary.file_count,
            diff_preview.summary.added_lines,
            diff_preview.summary.removed_lines
        ),
        files: diff_preview
            .files
            .iter()
            .map(|diff| SessionAppliedFileDiff {
                file_path: diff.file.display().to_string(),
                unified_diff_excerpt: render_code_diff_lines(diff).join("\n"),
            })
            .collect(),
        files_changed: diff_preview.summary.file_count,
        lines_added: diff_preview.summary.added_lines,
        lines_removed: diff_preview.summary.removed_lines,
    }
}

fn render_design_delta_diff(diff_preview: &crate::coding::UnifiedDiffPreview) -> String {
    let mut lines = Vec::new();
    for diff in &diff_preview.files {
        lines.extend(render_code_diff_lines(diff));
    }
    if lines.is_empty() {
        "(empty diff)".to_string()
    } else {
        lines.join("\n")
    }
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
        return "No tradeoff explanation available. Run alternative mutation search first."
            .to_string();
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
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::design_delta::{
        DesignDelta, MutationCandidate, MutationPlan, MutationSearchResult, MutationStrategy,
        RationalityScore,
    };
    use crate::nl::types::{CodingOptions, PlannedStep};
    use crate::service::dto::{ActionKind, IRActiveTransaction};
    use tempfile::tempdir;

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

        let output = execute_design_tradeoff_explanation_step(
            "設計トレードオフを要約して",
            &mut session,
            &mut conversation,
        );
        assert!(output.contains("Selected mutation: trait-extraction-1"));
        assert!(output.contains("Tradeoff summary:"));
        assert!(output.contains("Rejected:"));
        assert!(conversation.last_tradeoff_explanation.is_some());
    }

    #[test]
    fn rollback_current_transaction_clears_ir_state() {
        let mut conversation = ConversationState::default();
        conversation.last_target = Some(PathBuf::from("src/previous.rs"));
        conversation.set_active_transaction(IRActiveTransaction {
            transaction_id: "tx:src/coding.rs".to_string(),
            canonical_target: PathBuf::from("src/coding.rs"),
            pending: false,
            applied: true,
            validated: false,
            rollback_available: true,
            latest_diff_ref: None,
            latest_build_ok: None,
        });

        let output = execute_ir_rollback(&mut conversation);

        assert_eq!(output, "[rollback] reverted current IR transaction");
        assert!(conversation.active_transaction().is_none());
        assert_eq!(conversation.last_target, Some(PathBuf::from(".")));
        assert_eq!(
            conversation.ir_state.next_allowed_actions,
            vec![
                ActionKind::CodingPreview,
                ActionKind::Analyze,
                ActionKind::Refactor
            ]
        );
    }

    #[test]
    fn rollback_executor_route_is_direct() {
        let source = include_str!("executor.rs");
        let token_a = ["com", "pat"].concat();
        let token_b = ["sh", "im"].concat();
        let token_c = ["fall", "back route"].concat();
        assert!(!source.contains(&token_a));
        assert!(!source.contains(&token_b));
        assert!(!source.contains(&token_c));
    }

    #[test]
    fn design_delta_reasoning_executes_refactor_and_emits_diff() {
        let temp = tempdir().expect("tempdir");
        let root = temp.path();
        fs::create_dir_all(root.join("src")).expect("mkdir src");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"apply_cycle\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("write cargo");
        fs::write(
            root.join("src/lib.rs"),
            "pub mod renderer;\npub mod debug;\n",
        )
        .expect("write lib");
        fs::write(
            root.join("src/renderer.rs"),
            "use crate::debug;\npub fn render() {}\n",
        )
        .expect("write renderer");
        fs::write(
            root.join("src/debug.rs"),
            "use crate::renderer;\npub fn debug() {}\n",
        )
        .expect("write debug");

        let mut conversation = ConversationState::default();
        let delta = DesignDelta {
            workspace_root: root.to_path_buf(),
            impacted_crates: vec!["apply_cycle".to_string()],
            introduced_interfaces: vec!["renderer_debug_interface".to_string()],
            ..DesignDelta::default()
        };
        let output = execute_design_delta_refactor(
            "ExtractInterface(renderer -> debug) を適用して",
            &delta,
            &MutationPlan::default(),
            &mut conversation,
        );

        assert!(output.contains("DIFF"));
        assert!(output.contains("--- "));
        assert!(output.contains("RESULT"));
        assert!(output.contains("Refactoring applied successfully."));
        assert!(
            conversation
                .active_transaction()
                .and_then(|tx| tx.latest_diff_ref.as_ref())
                .is_some()
        );
    }
}
