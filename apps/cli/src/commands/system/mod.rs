use crate::command::{
    CommandError, CommandHandler, CommandPlugin, CommandRegistry, Output, SubCommandHandler,
};
use crate::session::AgentSession;
use crate::state::{Context, State};

pub struct SystemPlugin;

impl CommandPlugin for SystemPlugin {
    fn register(&self, registry: &mut CommandRegistry) {
        let mut cmd = CommandHandler::new("system");
        cmd.register_subcommand(SubCommandHandler::new("status", system_status));
        cmd.register_subcommand(SubCommandHandler::new("reset", system_reset));
        registry.register(cmd);
    }
}

/// /system status
///
/// 現在のセッション状態を表示する。
fn system_status(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    let _ = args;
    Ok(Output::text(format!(
        "State: {} | Mode: {:?} | History: {} entries | Tasks: {}",
        session.state.as_str(),
        session.mode,
        session.history.len(),
        session.tasks.len(),
    )))
}

/// /system reset
///
/// セッション状態（history / tasks / context）をリセットする。
fn system_reset(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    let _ = args;
    session.history.clear();
    session.tasks.clear();
    session.context = Context::default();
    session.current_plan = None;
    session.state = State::Idle;
    Ok(Output::text("Session reset."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        SystemPlugin.register(&mut registry);
        registry
    }

    #[test]
    fn system_status_shows_state() {
        let registry = build_registry();
        let mut session = AgentSession::new();
        let out = registry
            .execute("system", Some("status"), &[], &mut session)
            .unwrap();
        assert!(out.message.contains("idle"));
        assert!(out.message.contains("History: 0 entries"));
    }

    #[test]
    fn system_reset_clears_history() {
        let registry = build_registry();
        let mut session = AgentSession::new();
        session.record("input1");
        session.record("input2");
        assert_eq!(session.history.len(), 2);

        registry
            .execute("system", Some("reset"), &[], &mut session)
            .unwrap();
        assert_eq!(session.history.len(), 0);
    }

    #[test]
    fn system_status_reflects_history_count() {
        let registry = build_registry();
        let mut session = AgentSession::new();
        session.record("a");
        session.record("b");
        session.record("c");
        let out = registry
            .execute("system", Some("status"), &[], &mut session)
            .unwrap();
        assert!(out.message.contains("History: 3 entries"));
    }

    #[test]
    fn system_unknown_subcommand_returns_error() {
        let registry = build_registry();
        let mut session = AgentSession::new();
        let err = registry
            .execute("system", Some("nope"), &[], &mut session)
            .unwrap_err();
        assert!(matches!(
            err,
            crate::command::CommandError::UnknownSubcommand { .. }
        ));
    }
}
