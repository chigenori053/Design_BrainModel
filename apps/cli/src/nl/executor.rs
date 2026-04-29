use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::coding::RefactorRule;
use crate::ir::{
    ArtifactRef, ExecutionStatus, IRPersistenceArtifact, StepExecutionResultPayload,
    assert_accepted_plan_exists, emit_artifact_produced, emit_execution_result,
    emit_step_completed, emit_step_scheduled_with_id, emit_step_started, log_ir_bypass_warning,
    persist_ir_transition, store_execution_memory,
};
use crate::nl::execution_state::ExecutionState;
use crate::nl::session::ConversationState;
use crate::nl::types::{
    ExecutionPlan, ExecutionStage, Operation, PlannedStep, RefactorSpec, RepairSpec,
};
use crate::nl::validation::{validate_after_execution, validate_before_execution};
use crate::service::dto::ActionKind;
use crate::session::AgentSession;
use uuid::Uuid;

/// IR Execution Engine (DBM-PLAN-EXEC-STRUCT-SPEC v1.0)
///
/// Deterministic executor for ExecutionPlan.
/// Planner → ExecutionPlan（struct） → Executor — 文字列・JSON経路は存在しない。
#[allow(deprecated)]
pub fn execute_ir_plan(
    plan_id: Uuid,
    plan: &ExecutionPlan,
    session: &mut AgentSession,
    conversation: &mut ConversationState,
) -> Vec<String> {
    if let Err(err) = assert_accepted_plan_exists(&conversation.ir_state, plan_id) {
        log_ir_bypass_warning("execution attempted without plan_id");
        panic!("{err}");
    }

    let label = operation_label(&plan.operation);
    eprintln!("[EXECUTE] {}", label);
    eprintln!("[PLAN] {:?}", plan);
    eprintln!(
        "[TRACE_EXEC_PLAN]\noperation={label}\ntarget={}\nhas_effect={}",
        plan.target
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<none>".to_string()),
        plan.has_effect()
    );
    let mut execution_state = ExecutionState::new(plan_id, plan);

    let step_id = deterministic_step_id(plan_id, 0, &plan.operation);
    let step_id = emit_step_scheduled_with_id(&conversation.ir_state, step_id, plan_id, 0, label)
        .unwrap_or(step_id);

    // memory context は PlannedStep ベースの IR API を内部アダプタ経由で呼ぶ
    let ir_step = execution_plan_to_planned_step(plan);
    let _memory_context =
        crate::ir::query_memory_context(&conversation.ir_state, &ir_step, 0).unwrap_or_default();
    let _ = emit_step_started(&conversation.ir_state, step_id);
    if !plan.has_effect() {
        let output = "[NOOP]\nReason: plan has no executable effect".to_string();
        let _ = emit_step_completed(&conversation.ir_state, step_id, ExecutionStatus::Skipped);
        let payload = StepExecutionResultPayload {
            step_id,
            stdout: Some(output.clone()),
            stderr: None,
            structured_output: None,
            artifacts: Vec::new(),
        };
        let _ = emit_execution_result(&conversation.ir_state, payload.clone());
        let _ = store_execution_memory(&conversation.ir_state, &ir_step, 0, &payload);
        return vec![format!("[step 0] {label}\n{output}")];
    }
    execution_state.advance(ExecutionStage::Validate, "pre validation started");
    let pre_validation = validate_before_execution(plan);
    execution_state.set_validation(pre_validation.clone());
    if !pre_validation.is_valid {
        let violations = pre_validation
            .violations
            .iter()
            .map(|v| format!("{}: {}", v.code, v.message))
            .collect::<Vec<_>>()
            .join("\n");
        let _ = emit_step_completed(&conversation.ir_state, step_id, ExecutionStatus::Failure);
        let output = format!("[VALIDATION:before]\nExecution blocked\n{violations}");
        return vec![format!("[step 0] {label}\n{output}")];
    }
    execution_state.advance(ExecutionStage::Execute, "execution started");

    let target = plan.target.clone().unwrap_or_else(|| PathBuf::from("."));

    let output = match &plan.operation {
        Operation::Analyze => handle_analyze(&target, session, conversation),

        Operation::Validate => {
            let result = validate_before_execution(plan);
            if result.is_valid {
                "[VALIDATION] passed".to_string()
            } else {
                format!("[ERROR] validation failed: {:?}", result.violations)
            }
        }

        Operation::Composite(ops) => {
            execute_composite_plan(plan_id, plan, ops, session, conversation)
        }

        Operation::Refactor => {
            let request = plan.args.query.as_deref().unwrap_or("");
            if request.to_lowercase().contains("unused imports") || request.is_empty() {
                execute_refactor(RefactorRule::RemoveUnusedImports, &target, conversation)
            } else {
                let err = format!("ERROR: Unsupported refactor request: {request}");
                let _ =
                    emit_step_completed(&conversation.ir_state, step_id, ExecutionStatus::Failure);
                return vec![format!("[step 0] refactor\n{err}")];
            }
        }

        Operation::Repair => {
            let project = conversation.ir_state_manager.project_ir();
            let report = crate::consistency_engine::ConsistencyEngine::check(&project);
            let repair_input = crate::repair_engine::RepairInput { project, report };
            let repair_plan = crate::repair_engine::RepairEngine::build_plan(repair_input);
            match crate::repair_engine::RepairEngine::apply(
                &repair_plan,
                &mut conversation.ir_state_manager,
            ) {
                Ok(result) => {
                    let applied: Vec<String> =
                        result.applied.iter().map(|f| f.describe()).collect();
                    format!(
                        "[REPAIR] applied fixes for {}:\n{}",
                        target.display(),
                        applied.join("\n")
                    )
                }
                Err(err) => format!("[ERROR] repair failed: {err}"),
            }
        }

        Operation::Apply => {
            let project_ir = conversation.ir_state_manager.project_ir();
            let report = crate::consistency_engine::ConsistencyEngine::check(&project_ir);
            if !report.is_consistent {
                format!(
                    "Applied: false\nReason: consistency check failed\n{}",
                    report.render()
                )
            } else {
                execute_apply_previous_coding_step(conversation)
            }
        }

        Operation::Rollback => {
            conversation.rollback_current_transaction();
            "Rolled back current transaction".to_string()
        }

        Operation::Reload => execute_ir_reload_all(&target, conversation),

        Operation::NoOp => "[NOOP]".to_string(),
    };

    // §6.3 NOOP final guard: diff empty → skip post-validation and JSON parse path
    if output.trim().starts_with("[NOOP]") {
        let _ = emit_step_completed(&conversation.ir_state, step_id, ExecutionStatus::Skipped);
        let payload = StepExecutionResultPayload {
            step_id,
            stdout: Some(output.clone()),
            stderr: None,
            structured_output: None,
            artifacts: Vec::new(),
        };
        let _ = emit_execution_result(&conversation.ir_state, payload.clone());
        let _ = store_execution_memory(&conversation.ir_state, &ir_step, 0, &payload);
        return vec![format!("[step 0] {label}\n{output}")];
    }

    execution_state.set_output_diff(&output);
    execution_state.advance(ExecutionStage::PostValidate, "post validation started");
    let post_validation = validate_after_execution(plan, &output);
    execution_state.set_validation(post_validation.clone());
    let mut output = output;
    if !post_validation.is_valid {
        if plan.execution_metadata.rollback_required {
            conversation.rollback_current_transaction();
            execution_state.advance(ExecutionStage::PostValidate, "rollback completed");
            output = format!(
                "{output}\n[VALIDATION:after]\nExecution failed post validation; rollback completed"
            );
        } else {
            output = format!("{output}\n[VALIDATION:after]\nExecution failed post validation");
        }
    }

    update_state_after_operation(&plan.operation, &target, conversation);

    let success = !output.contains("ERROR:") && !output.contains("[ERROR]");
    let status = if success {
        ExecutionStatus::Success
    } else {
        ExecutionStatus::Failure
    };
    let _ = emit_step_completed(&conversation.ir_state, step_id, status);

    let mut artifacts = Vec::new();
    if output.contains("[DIFF]") {
        let artifact = ArtifactRef {
            artifact_kind: "code_diff".to_string(),
            artifact_id: format!("diff:{step_id}"),
            description: Some(format!("IR execution artifact for {}", label)),
        };
        let _ = emit_artifact_produced(&conversation.ir_state, artifact.clone());
        artifacts.push(artifact);
    }

    let payload = StepExecutionResultPayload {
        step_id,
        stdout: Some(output.clone()),
        stderr: None,
        structured_output: None,
        artifacts,
    };
    let _ = emit_execution_result(&conversation.ir_state, payload.clone());
    let _ = store_execution_memory(&conversation.ir_state, &ir_step, 0, &payload);

    vec![format!("[step 0] {label}\n{output}")]
}

/// ExecutionPlan.operation → PlannedStep（ir.rs の内部 API へのアダプタ）
fn execution_plan_to_planned_step(plan: &ExecutionPlan) -> PlannedStep {
    let target = plan.target.clone().unwrap_or_else(|| PathBuf::from("."));
    match &plan.operation {
        Operation::Analyze => PlannedStep::Analyze(target),
        Operation::Refactor => PlannedStep::Refactor(RefactorSpec {
            target,
            request: plan.args.query.clone().unwrap_or_default(),
        }),
        Operation::Repair => PlannedStep::Repair(RepairSpec { target }),
        Operation::Validate => PlannedStep::Validate(target),
        Operation::Composite(ops) => {
            let Some(first) = ops.first() else {
                return PlannedStep::Reload;
            };
            let mut child = plan.clone();
            child.operation = first.clone();
            return execution_plan_to_planned_step(&child);
        }
        Operation::Apply => PlannedStep::Apply,
        Operation::Rollback => PlannedStep::RollbackCurrentTransaction,
        Operation::Reload => PlannedStep::Reload,
        Operation::NoOp => PlannedStep::Reload,
    }
}

fn execute_composite_plan(
    plan_id: Uuid,
    plan: &ExecutionPlan,
    ops: &[Operation],
    session: &mut AgentSession,
    conversation: &mut ConversationState,
) -> String {
    let mut outputs = Vec::new();
    for (index, op) in ops.iter().enumerate() {
        let mut child = plan.clone();
        child.operation = op.clone();
        let child_step_id = deterministic_step_id(plan_id, index + 1, op);
        let label = operation_label(op);
        let _ = emit_step_scheduled_with_id(
            &conversation.ir_state,
            child_step_id,
            plan_id,
            index + 1,
            label,
        );
        let child_output = match op {
            Operation::Analyze => {
                let target = child.target.clone().unwrap_or_else(|| PathBuf::from("."));
                handle_analyze(&target, session, conversation)
            }
            Operation::Validate => {
                let validation = validate_before_execution(&child);
                if validation.is_valid {
                    "[VALIDATION] passed".to_string()
                } else {
                    format!("[ERROR] validation failed: {:?}", validation.violations)
                }
            }
            Operation::Refactor => {
                let target = child.target.clone().unwrap_or_else(|| PathBuf::from("."));
                let request = child.args.query.as_deref().unwrap_or("");
                if request.to_lowercase().contains("unused imports") || request.is_empty() {
                    execute_refactor(RefactorRule::RemoveUnusedImports, &target, conversation)
                } else {
                    format!("ERROR: Unsupported refactor request: {request}")
                }
            }
            Operation::Repair => {
                let target = child.target.clone().unwrap_or_else(|| PathBuf::from("."));
                update_state_after_operation(op, &target, conversation);
                "[REPAIR] composite repair scheduled".to_string()
            }
            Operation::Apply => execute_apply_previous_coding_step(conversation),
            Operation::Rollback => {
                conversation.rollback_current_transaction();
                "Rolled back current transaction".to_string()
            }
            Operation::Reload => {
                let target = child.target.clone().unwrap_or_else(|| PathBuf::from("."));
                execute_ir_reload_all(&target, conversation)
            }
            Operation::Composite(_) => "[ERROR] nested composite rejected".to_string(),
            Operation::NoOp => "[NOOP]".to_string(),
        };

        // §6.2 NOOP guard: diff empty → stop composite immediately
        if child_output.trim().starts_with("[NOOP]") {
            let _ = emit_step_completed(
                &conversation.ir_state,
                child_step_id,
                ExecutionStatus::Skipped,
            );
            outputs.push(format!("[composite step {index}] {label}\n{child_output}"));
            return outputs.join("\n");
        }

        let status = if child_output.contains("ERROR:") || child_output.contains("[ERROR]") {
            ExecutionStatus::Failure
        } else {
            ExecutionStatus::Success
        };
        let _ = emit_step_completed(&conversation.ir_state, child_step_id, status);
        outputs.push(format!("[composite step {index}] {label}\n{child_output}"));
        if status == ExecutionStatus::Failure {
            break;
        }
    }
    outputs.join("\n")
}

fn execute_refactor(
    rule: RefactorRule,
    path: &Path,
    conversation: &mut ConversationState,
) -> String {
    let Ok(before) = fs::read_to_string(path) else {
        return format!("[ERROR] failed to read file: {}", path.display());
    };
    let result = match rule {
        RefactorRule::RemoveUnusedImports => crate::coding::remove_unused_imports_refactor(&before),
    };
    match result {
        Ok(diff_result) => {
            if diff_result
                .diff
                .as_ref()
                .map(|diff| diff.trim().is_empty())
                .unwrap_or(true)
            {
                return "[NOOP]\nReason: No changes detected".to_string();
            }
            conversation.start_preview_transaction(path.to_path_buf());
            let diff = diff_result.diff.unwrap_or_default();
            if let Some(tx) = conversation.active_transaction_mut() {
                tx.latest_diff_ref = Some(crate::service::dto::SessionAppliedDiff {
                    summary: "remove unused imports".to_string(),
                    files: vec![crate::service::dto::SessionAppliedFileDiff {
                        file_path: path.display().to_string(),
                        unified_diff_excerpt: diff.clone(),
                    }],
                    files_changed: 1,
                    lines_added: 0,
                    lines_removed: diff_result.removed_lines,
                });
            }
            format!("[DIFF]\n{diff}")
        }
        Err(err) => format!("[ERROR] refactor failed: {err}"),
    }
}

fn handle_analyze(
    path: &Path,
    _session: &mut AgentSession,
    conversation: &mut ConversationState,
) -> String {
    eprintln!("[ANALYZE] {}", path.display());
    conversation.last_target = Some(path.to_path_buf());

    if path.is_file() {
        match fs::read_to_string(path) {
            Ok(content) => {
                let _ = crate::ir_state::reload(path, &mut conversation.ir_state_manager);
                content
            }
            Err(err) => format!("[ERROR] failed to read file: {}\n{}", path.display(), err),
        }
    } else if path.is_dir() {
        match conversation.ir_state_manager.build_project(path) {
            Ok(()) => "[IR] project built".to_string(),
            Err(err) => {
                format!(
                    "[ERROR] failed to analyze directory: {}\n{}",
                    path.display(),
                    err
                )
            }
        }
    } else {
        format!("[ERROR] path not found: {}", path.display())
    }
}

fn execute_apply_previous_coding_step(conversation: &mut ConversationState) -> String {
    let project_ir = conversation.ir_state_manager.project_ir();
    let report = crate::consistency_engine::ConsistencyEngine::check(&project_ir);
    if !report.is_consistent {
        return format!(
            "Applied: false\nReason: final consistency check failed\n{}",
            report.render()
        );
    }

    let before = conversation.ir_state.clone();
    let result = conversation.apply_transaction();
    match result {
        Ok(snapshot) => {
            let _ = persist_ir_transition(
                &before,
                &conversation.ir_state,
                ActionKind::Apply,
                "apply_previous_coding",
                IRPersistenceArtifact {
                    diff_ref: snapshot,
                    ..IRPersistenceArtifact::default()
                },
            );
            "Applied: true".to_string()
        }
        Err(err) => format!("Applied: false\nReason: {err}"),
    }
}

fn execute_ir_reload_all(root: &Path, conversation: &mut ConversationState) -> String {
    let had_pending = conversation.has_pending_transaction();
    conversation.clear_active_transaction();
    match conversation.ir_state_manager.reload_all(root) {
        Ok(()) => format!(
            "[IR_SYNC] reload all\nroot: {}\ndiff_invalidated: {had_pending}\ntracked: {}",
            root.display(),
            conversation.ir_state_manager.tracked_count()
        ),
        Err(err) => format!(
            "[IR_SYNC] reload all failed\nroot: {}\nreason: {err}",
            root.display()
        ),
    }
}

fn deterministic_step_id(plan_id: Uuid, index: usize, op: &Operation) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(plan_id.as_bytes());
    hasher.update(index.to_be_bytes());
    hasher.update(operation_label(op).as_bytes());
    let hash = hasher.finalize();
    Uuid::from_slice(&hash[..16]).unwrap()
}

fn update_state_after_operation(
    op: &Operation,
    target: &Path,
    conversation: &mut ConversationState,
) {
    match op {
        Operation::Analyze => {
            conversation.last_target = Some(target.to_path_buf());
            if target.is_dir() {
                let _ = conversation.ir_state_manager.build_project(target);
            } else {
                let _ = crate::ir_state::reload(target, &mut conversation.ir_state_manager);
            }
        }
        Operation::Refactor | Operation::Repair => {
            conversation.last_target = Some(target.to_path_buf());
        }
        Operation::Validate | Operation::Composite(_) => {}
        Operation::Apply | Operation::Rollback | Operation::Reload | Operation::NoOp => {}
    }
}

pub fn operation_label(op: &Operation) -> &'static str {
    match op {
        Operation::Analyze => "analyze",
        Operation::Refactor => "refactor",
        Operation::Validate => "validate",
        Operation::Composite(_) => "composite",
        Operation::Repair => "repair",
        Operation::Apply => "apply",
        Operation::Rollback => "rollback",
        Operation::Reload => "reload",
        Operation::NoOp => "noop",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::nl::types::{ExecutionPlan, Operation, PlanSource};

    fn make_plan_and_id(
        op: Operation,
        target: Option<PathBuf>,
        conversation: &mut ConversationState,
    ) -> (ExecutionPlan, uuid::Uuid) {
        let plan = ExecutionPlan::new(op, target, PlanSource::System);
        // Emit proposed + accepted using CommandPlan adapter (ir API still accepts CommandPlan)
        use crate::nl::types::CommandPlan;
        let cmd_plan = CommandPlan::from(&plan);
        let plan_id =
            crate::ir::emit_plan_proposed(&conversation.ir_state, cmd_plan, "test").unwrap();
        crate::ir::emit_plan_accepted(&conversation.ir_state, plan_id).unwrap();
        (plan, plan_id)
    }

    #[test]
    fn analyze_executes_directly() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = crate::ir::IRPersistenceStore::new(temp.path());
        let recovered = store.recover_or_create().unwrap();
        let mut session = AgentSession::with_root(temp.path().to_path_buf());
        let mut conversation = ConversationState::default();
        conversation.ir_state = recovered.state;
        let (plan, plan_id) = make_plan_and_id(
            Operation::Analyze,
            Some(PathBuf::from("src/lib.rs")),
            &mut conversation,
        );
        let output = execute_ir_plan(plan_id, &plan, &mut session, &mut conversation);
        // [ANALYZE] is emitted to stderr via eprintln!; the output vec contains the step label
        assert!(output.iter().any(|o| o.contains("[step 0] analyze")));
    }

    #[test]
    fn no_json_error_on_analyze() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = crate::ir::IRPersistenceStore::new(temp.path());
        let recovered = store.recover_or_create().unwrap();
        let mut session = AgentSession::with_root(temp.path().to_path_buf());
        let mut conversation = ConversationState::default();
        conversation.ir_state = recovered.state;
        let (plan, plan_id) = make_plan_and_id(
            Operation::Analyze,
            Some(PathBuf::from("src/lib.rs")),
            &mut conversation,
        );
        let output = execute_ir_plan(plan_id, &plan, &mut session, &mut conversation);
        for line in output {
            assert!(!line.contains("trailing characters"));
        }
    }

    #[test]
    fn refactor_no_diff_returns_noop_without_transaction() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("src.rs");
        fs::write(&file, "pub fn sample() {}\n").expect("write sample");
        let store = crate::ir::IRPersistenceStore::new(temp.path());
        let recovered = store.recover_or_create().unwrap();
        let mut session = AgentSession::with_root(temp.path().to_path_buf());
        let mut conversation = ConversationState::default();
        conversation.ir_state = recovered.state;
        let (plan, plan_id) = make_plan_and_id(Operation::Refactor, Some(file), &mut conversation);

        let output = execute_ir_plan(plan_id, &plan, &mut session, &mut conversation);

        assert!(output.iter().any(|line| line.contains("[NOOP]")));
        assert!(!conversation.has_pending_transaction());
    }

    // §10.1 — No-op 時に execution が走らない
    #[test]
    fn no_diff_does_not_execute() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("src.rs");
        fs::write(&file, "pub fn sample() {}\n").expect("write sample");
        let store = crate::ir::IRPersistenceStore::new(temp.path());
        let recovered = store.recover_or_create().unwrap();
        let mut session = AgentSession::with_root(temp.path().to_path_buf());
        let mut conversation = ConversationState::default();
        conversation.ir_state = recovered.state;
        let (plan, plan_id) = make_plan_and_id(Operation::Refactor, Some(file), &mut conversation);

        let output = execute_ir_plan(plan_id, &plan, &mut session, &mut conversation);
        let joined = output.join("\n");

        assert!(joined.contains("[NOOP]"), "expected [NOOP] in: {joined}");
        assert!(
            !joined.contains("Execution failed"),
            "unexpected error in: {joined}"
        );
    }

    // §10.2 — No-op 時に trailing characters が発生しない
    #[test]
    fn no_trailing_characters_on_noop() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("src.rs");
        fs::write(&file, "pub fn sample() {}\n").expect("write sample");
        let store = crate::ir::IRPersistenceStore::new(temp.path());
        let recovered = store.recover_or_create().unwrap();
        let mut session = AgentSession::with_root(temp.path().to_path_buf());
        let mut conversation = ConversationState::default();
        conversation.ir_state = recovered.state;
        let (plan, plan_id) = make_plan_and_id(Operation::Refactor, Some(file), &mut conversation);

        let output = execute_ir_plan(plan_id, &plan, &mut session, &mut conversation);

        for line in &output {
            assert!(
                !line.contains("trailing characters"),
                "trailing characters in: {line}"
            );
        }
    }

    // §10.3 — Determinism: 同じ入力で同じ出力
    #[test]
    fn noop_is_deterministic() {
        let temp = tempfile::tempdir().expect("tempdir");
        let file = temp.path().join("src.rs");
        fs::write(&file, "pub fn sample() {}\n").expect("write sample");

        let run = || {
            let store = crate::ir::IRPersistenceStore::new(temp.path());
            let recovered = store.recover_or_create().unwrap();
            let mut session = AgentSession::with_root(temp.path().to_path_buf());
            let mut conversation = ConversationState::default();
            conversation.ir_state = recovered.state;
            let (plan, plan_id) =
                make_plan_and_id(Operation::Refactor, Some(file.clone()), &mut conversation);
            execute_ir_plan(plan_id, &plan, &mut session, &mut conversation).join("\n")
        };

        let r1 = run();
        let r2 = run();
        assert_eq!(r1, r2, "noop output must be deterministic");
    }
}
