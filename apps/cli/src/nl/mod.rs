pub mod autonomous;
pub mod context;
pub mod convergence;
pub mod execution_state;
pub mod executor;
pub mod goal;
pub mod intent;
pub mod intent_ranker;
pub mod language;
pub mod language_detection;
pub mod language_intent_bridge;
pub mod r#loop;
pub mod multilingual_router;
pub mod normalization;
pub mod planner;
pub mod planner_v2;
pub mod runtime_intent;
pub mod session;
pub mod target;
pub mod types;
pub mod validation;

use crate::mlaal;
use crate::session::AgentSession;

use self::session::ConversationState;
use self::types::{CommandPlan, ExecutionPlan};

pub use execution_state::{DiffSnapshot, ExecutionEvent, ExecutionState};
#[allow(deprecated)]
pub use executor::execute_ir_plan;
pub use planner::to_runtime_plan;
pub use planner_v2::{plan_input, update_conversation_after_plan};
pub use validation::{ValidationResult, Violation};

pub fn resolve_command_plan(
    input: &str,
    session: &AgentSession,
    conversation: &ConversationState,
) -> (Option<ExecutionPlan>, &'static str) {
    mlaal::resolve_command_plan_with_compatibility(input, session, conversation)
}

pub fn render_plan_summary_with_label(plan: &ExecutionPlan, planner_label: &str) -> String {
    use crate::nl::executor::operation_label;
    let op = operation_label(&plan.operation);
    let target_str = plan
        .target
        .as_ref()
        .map(|p| format!(" → {}", p.display()))
        .unwrap_or_default();
    format!("[PLAN:{planner_label}]\n1. {op}{target_str}")
}

/// CommandPlan用レンダラー（内部後方互換）
#[allow(dead_code)]
pub(crate) fn render_command_plan_summary(plan: &CommandPlan, planner_label: &str) -> String {
    let steps = plan
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| format!("{}. {:?}", index + 1, step))
        .collect::<Vec<_>>()
        .join("\n");
    format!("[PLAN:{planner_label}]\n{steps}")
}
