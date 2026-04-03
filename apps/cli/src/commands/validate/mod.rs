use crate::command::{CommandError, CommandHandler, CommandPlugin, CommandRegistry, Output};
use crate::commands::path_resolver::resolve_command_path;
use crate::session::AgentSession;

pub struct ValidatePlugin;

impl CommandPlugin for ValidatePlugin {
    fn register(&self, registry: &mut CommandRegistry) {
        let mut cmd = CommandHandler::new("validate");
        cmd.set_default(execute);
        registry.register(cmd);
    }
}

/// /validate [path]
///
/// アーキテクチャ検証を実行する。パス省略時はセッションの last_path を使用。
fn execute(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    let path = resolve_and_store_path(args, session)?;
    crate::nl_executor::run_design_command("validate", &[path])
        .map(Output::text)
        .map_err(CommandError::ExecutionError)
}

fn resolve_and_store_path(
    args: &[String],
    session: &mut AgentSession,
) -> Result<String, CommandError> {
    eprintln!("TRACE:V1:ENTER");
    eprintln!("TRACE:V2:LAST={:?}", session.context.last_path.clone());

    let path = resolve_command_path(args, session).map_err(CommandError::ExecutionError)?;
    eprintln!("TRACE:V3:RETURN={:?}", path);

    let path = path.display().to_string();
    session.context.set_last_path(&path);
    session.context.last_command = Some("validate".to_string());
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        ValidatePlugin.register(&mut registry);
        registry
    }

    #[test]
    fn validate_is_registered() {
        let registry = build_registry();
        assert!(registry.command_names().contains(&"validate"));
    }

    #[test]
    fn validate_stores_last_path_in_session() {
        let mut session = AgentSession::new();
        let path = resolve_and_store_path(&["src/lib.rs".to_string()], &mut session).unwrap();
        assert_eq!(path, "src/lib.rs");
        assert_eq!(session.context.last_path, Some("src/lib.rs".to_string()));
        assert_eq!(session.context.last_command, Some("validate".to_string()));
    }

    #[test]
    fn validate_falls_back_to_last_path() {
        let temp = std::env::temp_dir().join(format!("dbm-validate-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&temp).expect("create temp dir");
        let file_path = temp.join("sample.rs");
        std::fs::write(&file_path, "fn main() {}\n").expect("write sample file");

        let mut session = AgentSession::new();
        session
            .context
            .set_last_path(file_path.to_str().expect("utf-8 path"));

        let path = resolve_and_store_path(&[], &mut session).expect("fallback path should resolve");
        assert_eq!(
            path,
            file_path
                .canonicalize()
                .expect("temp file path should canonicalize")
                .display()
                .to_string()
        );
        assert_eq!(session.context.last_command, Some("validate".to_string()));
        std::fs::remove_dir_all(&temp).expect("cleanup temp dir");
    }

    #[test]
    fn validate_fails_fast_for_missing_last_path() {
        let mut session = AgentSession::new();
        session
            .context
            .set_last_path("/definitely/missing/dbm-validate-fallback.rs");

        let err =
            resolve_and_store_path(&[], &mut session).expect_err("missing path should fail fast");
        match err {
            CommandError::ExecutionError(message) => assert!(message.contains("MissingLastPath")),
            other => panic!("unexpected error variant: {other:?}"),
        }
    }
}
