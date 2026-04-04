pub mod autonomous;
pub mod context;
pub mod convergence;
pub mod executor;
pub mod goal;
pub mod intent;
pub mod planner;
pub mod planner_v2;
pub mod session;
pub mod target;
pub mod types;

use crate::session::AgentSession;

use self::session::ConversationState;
use self::types::CommandPlan;

pub use executor::{execute_plan, render_plan_summary, render_plan_summary_with_label};
pub use planner::{plan_input, to_legacy_plan};

pub fn plan_input_with_v2_fallback(
    input: &str,
    session: &AgentSession,
    conversation: &ConversationState,
) -> (Option<CommandPlan>, &'static str) {
    let command_plan_v2 = planner_v2::plan_input(input, session, conversation);
    let planner_label = if command_plan_v2.is_some() {
        "nl_v2"
    } else {
        "nl_fallback"
    };
    (
        command_plan_v2.or_else(|| planner::plan_input(input, session)),
        planner_label,
    )
}
