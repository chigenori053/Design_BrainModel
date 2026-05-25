pub mod log;
pub mod maintenance;

use crate::command::{
    CommandError, CommandHandler, CommandPlugin, CommandRegistry, Output, SubCommandHandler,
};
use crate::session::AgentSession;

pub struct MemoryPlugin;

impl CommandPlugin for MemoryPlugin {
    fn register(&self, registry: &mut CommandRegistry) {
        let mut cmd = CommandHandler::new("memory");
        cmd.register_subcommand(SubCommandHandler::new("import", import));
        cmd.register_subcommand(SubCommandHandler::new("log", |args, _session| {
            log::handle_log(args)
        }));
        cmd.register_subcommand(SubCommandHandler::new(
            "maintenance",
            maintenance::handle_maintenance,
        ));
        registry.register(cmd);
    }
}

/// NL pipeline を経由せず memory サブコマンドを直接処理するディスパッチャ。
///
/// main.rs の早期 dispatch ブロックから呼ばれる。`args` は `["maintenance", ...]` 形式で渡す。
pub fn dispatch_memory_command(args: &[String]) -> Result<Output, CommandError> {
    let mut session = AgentSession::new();
    match args.first().map(String::as_str) {
        Some("log") => log::handle_log(&args[1..]),
        Some("maintenance") => maintenance::handle_maintenance(&args[1..], &mut session),
        Some(other) => Err(CommandError::UnknownSubcommand {
            command: "memory".to_string(),
            subcommand: other.to_string(),
        }),
        None => Ok(Output::text(
            "memory: subcommand required.\nAvailable: log, maintenance\n",
        )),
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

    fn temp_store_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{}_{}.json",
            name,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
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

    /// dispatch_memory_command が NL pipeline を経由せず maintenance を直接処理することを確認する。
    /// NL pipeline に流れた場合は ClarificationRequired エラーになるが、
    /// 直接ルートでは dedup の空ストア結果が返る。
    #[test]
    fn memory_maintenance_dedup_routes_without_nl_pipeline() {
        let tmp = temp_store_path("route_test");
        let args = vec![
            "maintenance".to_string(),
            "dedup".to_string(),
            "--dry-run".to_string(),
            "--store".to_string(),
            tmp.display().to_string(),
        ];
        let out = dispatch_memory_command(&args).unwrap();
        assert!(!out.message.contains("ClarificationRequired"));
        assert!(
            out.message.contains("dedup")
                || out.message.contains("空")
                || out.message.contains("Dedup")
                || out.message.contains("DBM Memory")
        );
    }

    /// 未知のサブコマンドに対して structured error が返ることを確認する。
    #[test]
    fn memory_maintenance_unknown_subcommand_returns_structured_error() {
        let args = vec!["maintenance".to_string(), "nonexistent_cmd".to_string()];
        let err = dispatch_memory_command(&args).unwrap_err();
        assert!(matches!(err, CommandError::UnknownSubcommand { .. }));
    }
}
