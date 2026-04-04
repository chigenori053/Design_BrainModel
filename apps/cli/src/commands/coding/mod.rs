use crate::command::{CommandError, CommandHandler, CommandPlugin, CommandRegistry, Output};
use crate::commands::path_resolver::resolve_command_path;
use crate::session::AgentSession;
use std::path::PathBuf;

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
    let (path, extra) = resolve_and_store_path(args, session, "coding")?;
    let mut cli_args = vec![path];
    cli_args.extend(extra);
    crate::nl_executor::run_design_command("coding", &cli_args)
        .map(Output::text)
        .map_err(CommandError::ExecutionError)
}

/// /diff [path]  ─ 変更の差分を表示する
fn execute_diff(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    let (path, extra) = resolve_and_store_path(args, session, "diff")?;
    let mut cli_args = vec![path];
    cli_args.extend(extra);
    crate::nl_executor::run_design_command("diff", &cli_args)
        .map(Output::text)
        .map_err(CommandError::ExecutionError)
}

/// /check [path]  ─ 変更をドライランで検証する
fn execute_check(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    let (path, extra) = resolve_and_store_path(args, session, "check")?;
    let mut cli_args = vec![path];
    cli_args.extend(extra);
    crate::nl_executor::run_design_command("check", &cli_args)
        .map(Output::text)
        .map_err(CommandError::ExecutionError)
}

/// /apply [path]  ─ 変更を実際に適用する
fn execute_apply(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    let (path, extra) = resolve_and_store_path(args, session, "apply")?;
    let mut cli_args = vec![path];
    cli_args.extend(extra);
    crate::nl_executor::run_design_command("apply", &cli_args)
        .map(Output::text)
        .map_err(CommandError::ExecutionError)
}

fn resolve_and_store_path(
    args: &[String],
    session: &mut AgentSession,
    command_name: &str,
) -> Result<(String, Vec<String>), CommandError> {
    let path = resolve_path(args, session).map_err(CommandError::ExecutionError)?;
    let path = path.display().to_string();
    session.context.set_last_path(&path);
    session.context.last_command = Some(command_name.to_string());
    Ok((path, args.iter().skip(1).cloned().collect()))
}

fn resolve_path(args: &[String], session: &AgentSession) -> Result<PathBuf, String> {
    resolve_command_path(args, session)
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
        let (path, extra) =
            resolve_and_store_path(&["src/lib.rs".to_string()], &mut session, "coding").unwrap();
        assert_eq!(path, "src/lib.rs");
        assert!(extra.is_empty());
        assert_eq!(session.context.last_path, Some("src/lib.rs".to_string()));
        assert_eq!(session.context.last_command, Some("coding".to_string()));
    }

    #[test]
    fn diff_uses_last_path_fallback() {
        let temp = std::env::temp_dir().join(format!("dbm-coding-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp).expect("create temp dir");
        let file_path = temp.join("sample.rs");
        std::fs::write(&file_path, "fn main() {}\n").expect("write sample file");

        let mut session = AgentSession::new();
        session
            .context
            .set_last_path(file_path.to_str().expect("utf-8 path"));

        let path = resolve_path(&[], &session).expect("fallback path should resolve");
        assert_eq!(
            path,
            file_path
                .canonicalize()
                .expect("temp file path should canonicalize")
        );
        std::fs::remove_dir_all(&temp).expect("cleanup temp dir");
    }

    #[test]
    fn diff_fails_fast_for_missing_last_path() {
        let mut session = AgentSession::new();
        session
            .context
            .set_last_path("/definitely/missing/dbm-coding-fallback.rs");

        let err = resolve_path(&[], &session).expect_err("missing path should fail fast");
        assert!(err.contains("MissingLastPath"));
    }
}
