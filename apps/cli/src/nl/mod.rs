pub mod autonomous;
pub mod context;
pub mod convergence;
pub mod executor;
pub mod goal;
pub mod intent;
pub mod intent_ranker;
pub mod language_detection;
pub mod language_intent_bridge;
pub mod r#loop;
pub mod multilingual_router;
pub mod planner;
pub mod planner_v2;
pub mod session;
pub mod target;
pub mod types;

use crate::mlaal;
use crate::session::AgentSession;

use self::session::ConversationState;
use self::types::CommandPlan;

pub use executor::{execute_plan, render_plan_summary, render_plan_summary_with_label};
pub use planner::{plan_input, to_runtime_plan};
pub use planner_v2::update_conversation_after_plan;

pub fn resolve_command_plan(
    input: &str,
    session: &AgentSession,
    conversation: &ConversationState,
) -> (Option<CommandPlan>, &'static str) {
    mlaal::resolve_command_plan_with_compatibility(input, session, conversation)
}
