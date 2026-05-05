use std::path::Path;

use crate::command::{
    CommandError, CommandHandler, CommandPlugin, CommandRegistry, Output, SubCommandHandler,
};
use crate::execution_foundation::{ExecAction, ExecutionFoundation, format_exec_report};
use crate::session::AgentSession;

const DEFAULT_TIMEOUT_MS: u64 = 60_000;

pub struct ExecPlugin;

impl CommandPlugin for ExecPlugin {
    fn register(&self, registry: &mut CommandRegistry) {
        let mut cmd = CommandHandler::new("exec");
        cmd.register_subcommand(SubCommandHandler::new("detect", detect));
        cmd.register_subcommand(SubCommandHandler::new("install", install));
        cmd.register_subcommand(SubCommandHandler::new("build", build));
        cmd.register_subcommand(SubCommandHandler::new("test", test));
        cmd.register_subcommand(SubCommandHandler::new("run", run));
        registry.register(cmd);
    }
}

fn detect(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    execute_action(ExecAction::Detect, args, session)
}

fn install(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    execute_action(ExecAction::Install, args, session)
}

fn build(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    execute_action(ExecAction::Build, args, session)
}

fn test(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    execute_action(ExecAction::Test, args, session)
}

fn run(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    execute_action(ExecAction::Run, args, session)
}

fn execute_action(
    action: ExecAction,
    args: &[String],
    session: &mut AgentSession,
) -> Result<Output, CommandError> {
    let path = args
        .first()
        .map(|s| s.as_str())
        .unwrap_or_else(|| session.context.last_path_or_default())
        .to_string();
    session.context.set_last_path(&path);
    session.context.last_command = Some(format!("exec {}", action.as_str()));

    ExecutionFoundation::execute(Path::new(&path), action, DEFAULT_TIMEOUT_MS)
        .map(|report| Output::text(format_exec_report(&report)))
        .map_err(CommandError::ExecutionError)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        ExecPlugin.register(&mut registry);
        registry
    }

    #[test]
    fn exec_is_registered() {
        let registry = build_registry();
        assert!(registry.command_names().contains(&"exec"));
    }

    #[test]
    fn exec_subcommands_are_registered() {
        let mut handler = CommandHandler::new("exec");
        handler.register_subcommand(SubCommandHandler::new("build", build));
        handler.register_subcommand(SubCommandHandler::new("test", test));
        assert_eq!(handler.subcommand_names(), vec!["build", "test"]);
    }

    #[test]
    fn build_stores_last_path() {
        let mut session = AgentSession::new();
        let _ = build(&[".".to_string()], &mut session);
        assert_eq!(session.context.last_path, None);
        assert_eq!(session.context.last_command, Some("exec build".to_string()));
    }
}
