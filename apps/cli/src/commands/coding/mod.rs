use crate::command::{CommandError, CommandHandler, CommandPlugin, CommandRegistry, Output};
use crate::session::AgentSession;

pub struct CodingPlugin;

impl CommandPlugin for CodingPlugin {
    fn register(&self, registry: &mut CommandRegistry) {
        // coding
        let mut coding_cmd = CommandHandler::new("coding");
        coding_cmd.set_default(execute_coding);
        registry.register(coding_cmd);

        // diff
        let mut diff_cmd = CommandHandler::new("diff");
        diff_cmd.set_default(execute_diff);
        registry.register(diff_cmd);

        // check
        let mut check_cmd = CommandHandler::new("check");
        check_cmd.set_default(execute_check);
        registry.register(check_cmd);

        // apply
        let mut apply_cmd = CommandHandler::new("apply");
        apply_cmd.set_default(execute_apply);
        registry.register(apply_cmd);
    }
}

/// /coding [path]  ─ コード変更セットを生成する
fn execute_coding(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    let path = resolve_path(args, session);
    session.context.set_last_path(&path);
    session.context.last_command = Some("coding".to_string());
    crate::nl_executor::run_design_command("coding", &[path])
        .map(Output::text)
        .map_err(CommandError::ExecutionError)
}

/// /diff [path]  ─ 変更の差分を表示する
fn execute_diff(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    let path = resolve_path(args, session);
    session.context.set_last_path(&path);
    session.context.last_command = Some("diff".to_string());
    crate::nl_executor::run_design_command("diff", &[path])
        .map(Output::text)
        .map_err(CommandError::ExecutionError)
}

/// /check [path]  ─ 変更をドライランで検証する
fn execute_check(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    let path = resolve_path(args, session);
    session.context.set_last_path(&path);
    session.context.last_command = Some("check".to_string());
    crate::nl_executor::run_design_command("check", &[path])
        .map(Output::text)
        .map_err(CommandError::ExecutionError)
}

/// /apply [path]  ─ 変更を実際に適用する
fn execute_apply(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    let path = resolve_path(args, session);
    session.context.set_last_path(&path);
    session.context.last_command = Some("apply".to_string());
    crate::nl_executor::run_design_command("apply", &[path, "--apply".to_string()])
        .map(Output::text)
        .map_err(CommandError::ExecutionError)
}

fn resolve_path(args: &[String], session: &AgentSession) -> String {
    args.first()
        .map(|s| s.as_str())
        .unwrap_or_else(|| session.context.last_path_or_default())
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        CodingPlugin.register(&mut registry);
        registry
    }

    #[test]
    fn coding_commands_are_registered() {
        let registry = build_registry();
        let names = registry.command_names();
        assert!(names.contains(&"coding"));
        assert!(names.contains(&"diff"));
        assert!(names.contains(&"check"));
        assert!(names.contains(&"apply"));
    }

    #[test]
    fn coding_stores_last_path() {
        let mut session = AgentSession::new();
        let _ = execute_coding(&["src/lib.rs".to_string()], &mut session);
        assert_eq!(session.context.last_path, Some("src/lib.rs".to_string()));
        assert_eq!(session.context.last_command, Some("coding".to_string()));
    }

    #[test]
    fn diff_uses_last_path_fallback() {
        let mut session = AgentSession::new();
        session.context.set_last_path("src/main.rs");
        let _ = execute_diff(&[], &mut session);
        assert_eq!(session.context.last_command, Some("diff".to_string()));
    }
}
