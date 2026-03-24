use crate::command::{CommandError, CommandHandler, CommandPlugin, CommandRegistry, Output};
use crate::session::AgentSession;

pub struct RefactorPlugin;

impl CommandPlugin for RefactorPlugin {
    fn register(&self, registry: &mut CommandRegistry) {
        let mut cmd = CommandHandler::new("refactor");
        cmd.set_default(execute);
        registry.register(cmd);
    }
}

/// /refactor [path]
///
/// リファクタリング案を生成する。パス省略時はセッションの last_path を使用。
fn execute(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    let path = args
        .first()
        .map(|s| s.as_str())
        .unwrap_or_else(|| session.context.last_path_or_default())
        .to_string();
    session.context.set_last_path(&path);
    session.context.last_command = Some("refactor".to_string());
    crate::nl_executor::run_design_command("refactor", &[path])
        .map(Output::text)
        .map_err(CommandError::ExecutionError)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        RefactorPlugin.register(&mut registry);
        registry
    }

    #[test]
    fn refactor_is_registered() {
        let registry = build_registry();
        assert!(registry.command_names().contains(&"refactor"));
    }

    #[test]
    fn refactor_stores_last_path_in_session() {
        let mut session = AgentSession::new();
        let _ = execute(&["src/lib.rs".to_string()], &mut session);
        assert_eq!(session.context.last_path, Some("src/lib.rs".to_string()));
        assert_eq!(session.context.last_command, Some("refactor".to_string()));
    }
}
