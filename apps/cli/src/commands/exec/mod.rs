use crate::command::{CommandError, CommandHandler, CommandPlugin, CommandRegistry, Output};
use crate::session::AgentSession;

pub struct ExecPlugin;

impl CommandPlugin for ExecPlugin {
    fn register(&self, registry: &mut CommandRegistry) {
        let mut cmd = CommandHandler::new("run");
        cmd.set_default(execute);
        registry.register(cmd);
    }
}

/// /run [path]
///
/// ファイルをサンドボックスで実行する。パス省略時はセッションの last_path を使用。
fn execute(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    let path = args
        .first()
        .map(|s| s.as_str())
        .unwrap_or_else(|| session.context.last_path_or_default())
        .to_string();
    session.context.set_last_path(&path);
    session.context.last_command = Some("run".to_string());
    crate::nl_executor::run_design_command("run", &[path])
        .map(Output::text)
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
    fn run_is_registered() {
        let registry = build_registry();
        assert!(registry.command_names().contains(&"run"));
    }

    #[test]
    fn run_stores_last_path() {
        let mut session = AgentSession::new();
        let _ = execute(&["main.rs".to_string()], &mut session);
        assert_eq!(session.context.last_path, Some("main.rs".to_string()));
        assert_eq!(session.context.last_command, Some("run".to_string()));
    }
}
