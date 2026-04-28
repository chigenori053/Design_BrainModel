use std::path::PathBuf;

use crate::nl::context::merge_target;
use crate::nl::intent::{wants_analyze, wants_coding};
use crate::nl::session::ConversationState;
use crate::nl::types::{
    ExecutionMetadata, ExecutionPlan, ExecutionStage, IntentType, Operation, PlanArgs,
    PlanMetadata, PlanSource, ResolvedTarget, ValidationPolicy,
};
use crate::session::AgentSession;

/// Synthesizes IR steps from inferred intent.
pub fn synthesize_steps_from_intent(
    _intent_type: &IntentType,
    input: &str,
    merged: &ResolvedTarget,
    _coding_intent: Option<()>,
) -> Option<ExecutionPlan> {
    match _intent_type {
        IntentType::Analyze | IntentType::AnalyzeArchitecture => Some(ExecutionPlan::new(
            Operation::Analyze,
            Some(workspace_path(merged)),
            PlanSource::ReplInput,
        )),
        IntentType::Coding | IntentType::CodingEdit => Some(ExecutionPlan {
            operation: Operation::Refactor,
            target: Some(workspace_path(merged)),
            args: PlanArgs {
                query: Some(input.to_string()),
                flags: vec![],
            },
            metadata: PlanMetadata::with_source(PlanSource::ReplInput),
            validation_policy: ValidationPolicy::default(),
            execution_stages: vec![
                ExecutionStage::Plan,
                ExecutionStage::Validate,
                ExecutionStage::Execute,
                ExecutionStage::PostValidate,
            ],
            execution_metadata: ExecutionMetadata::default(),
        }),
        IntentType::Repair => Some(ExecutionPlan::new(
            Operation::Repair,
            Some(workspace_path(merged)),
            PlanSource::ReplInput,
        )),
        _ => None,
    }
}

pub fn plan_input(
    input: &str,
    session: &AgentSession,
    conversation: &ConversationState,
) -> Option<ExecutionPlan> {
    let input = input.trim().replace('\u{00A0}', " ");
    let input = input.as_str();
    let lower = input.to_lowercase();
    let merged = merge_target(input, session, conversation);

    // Apply (IR promotion)
    if is_explicit_apply_intent(input) {
        return Some(ExecutionPlan::new(
            Operation::Apply,
            None,
            PlanSource::ReplInput,
        ));
    }

    // Reload
    if lower.trim() == "reload" || lower.contains("ir reload") || lower == "再同期" {
        return Some(ExecutionPlan::new(
            Operation::Reload,
            Some(workspace_path(&merged)),
            PlanSource::ReplInput,
        ));
    }

    // Analyze project
    if lower.contains("whole project")
        || lower.contains("analyze project")
        || lower.contains("project .")
    {
        return Some(ExecutionPlan::new(
            Operation::Analyze,
            Some(workspace_path(&merged)),
            PlanSource::ReplInput,
        ));
    }

    // Repair
    if lower.contains("repair") || lower.contains("修正") || lower.contains("fix") {
        return Some(ExecutionPlan::new(
            Operation::Repair,
            Some(workspace_path(&merged)),
            PlanSource::ReplInput,
        ));
    }

    // Refactor (with optional analyze-first — executor handles the analysis internally)
    if wants_coding(&lower) || lower.contains("refactor") {
        return Some(ExecutionPlan {
            operation: Operation::Refactor,
            target: Some(workspace_path(&merged)),
            args: PlanArgs {
                query: Some(input.to_string()),
                flags: vec![],
            },
            metadata: PlanMetadata::with_source(PlanSource::ReplInput),
            validation_policy: ValidationPolicy::default(),
            execution_stages: vec![
                ExecutionStage::Plan,
                ExecutionStage::Validate,
                ExecutionStage::Execute,
                ExecutionStage::PostValidate,
            ],
            execution_metadata: ExecutionMetadata::default(),
        });
    }

    // Analyze
    if wants_analyze(&lower) {
        return Some(ExecutionPlan::new(
            Operation::Analyze,
            Some(workspace_path(&merged)),
            PlanSource::ReplInput,
        ));
    }

    None
}

fn workspace_path(info: &ResolvedTarget) -> PathBuf {
    info.path.clone()
}

fn is_explicit_apply_intent(input: &str) -> bool {
    let lower = input.to_lowercase();
    lower.contains("--apply") || lower == "apply" || lower == "適用"
}

pub fn update_conversation_after_plan(
    _input: &str,
    plan: &ExecutionPlan,
    conversation: &mut ConversationState,
) {
    conversation.last_plan = Some(plan.clone());
    if let Some(path) = plan.target.clone() {
        conversation.note_target(path);
    }
}
