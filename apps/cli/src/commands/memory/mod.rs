mod maintenance;

use crate::command::{
    CommandError, CommandHandler, CommandPlugin, CommandRegistry, Output, SubCommandHandler,
};
use crate::session::AgentSession;

pub struct MemoryPlugin;

impl CommandPlugin for MemoryPlugin {
    fn register(&self, registry: &mut CommandRegistry) {
        let mut cmd = CommandHandler::new("memory");
        cmd.register_subcommand(SubCommandHandler::new("import", import));
        cmd.register_subcommand(SubCommandHandler::new(
            "maintenance",
            maintenance::handle_maintenance,
        ));
        registry.register(cmd);
    }
}

/// /memory import <path> [--verbose]
///
/// シードJSONをメモリにインポートする。
fn import(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let mut cli_args = vec!["import".to_string()];
    cli_args.extend_from_slice(args);
    crate::nl_executor::run_design_command("memory", &cli_args)
        .map(Output::text)
        .map_err(CommandError::ExecutionError)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        MemoryPlugin.register(&mut registry);
        registry
    }

    #[test]
    fn memory_is_registered() {
        let registry = build_registry();
        assert!(registry.command_names().contains(&"memory"));
    }

    #[test]
    fn memory_without_subcommand_lists_available() {
        let registry = build_registry();
        let mut session = AgentSession::new();
        let out = registry.execute("memory", None, &[], &mut session).unwrap();
        assert!(out.message.contains("import") || out.message.contains("Available"));
    }
}
