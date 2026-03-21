use std::collections::HashMap;

use crate::session::AgentSession;

use super::{CommandError, CommandHandler, Output};

/// CommandRegistry
///
/// すべての Command はここに登録され、ここ経由で実行される。
/// HashMap による O(1) lookup を保証する。
pub struct CommandRegistry {
    commands: HashMap<String, CommandHandler>,
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    /// CommandHandler を登録する
    pub fn register(&mut self, handler: CommandHandler) {
        self.commands.insert(handler.name.clone(), handler);
    }

    /// name に対応する Command を実行する
    pub fn execute(
        &self,
        name: &str,
        subcommand: Option<&str>,
        args: &[String],
        session: &mut AgentSession,
    ) -> Result<Output, CommandError> {
        let handler = self
            .commands
            .get(name)
            .ok_or_else(|| CommandError::UnknownCommand(name.to_string()))?;
        handler.execute(subcommand, args, session)
    }

    /// 登録済みコマンド名の一覧（ソート済み）
    pub fn command_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.commands.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }

    /// コマンドが登録されているか確認する
    pub fn contains(&self, name: &str) -> bool {
        self.commands.contains_key(name)
    }
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::SubCommandHandler;
    use crate::session::AgentSession;

    fn make_registry() -> CommandRegistry {
        let mut reg = CommandRegistry::new();
        let mut cmd = CommandHandler::new("ping");
        cmd.register_subcommand(SubCommandHandler::new("pong", |_, _| {
            Ok(Output::text("pong"))
        }));
        reg.register(cmd);
        reg
    }

    #[test]
    fn register_and_execute_known_command() {
        let reg = make_registry();
        let mut session = AgentSession::new();
        let out = reg
            .execute("ping", Some("pong"), &[], &mut session)
            .unwrap();
        assert_eq!(out.message, "pong");
    }

    #[test]
    fn unknown_command_returns_error() {
        let reg = make_registry();
        let mut session = AgentSession::new();
        let err = reg.execute("nope", None, &[], &mut session).unwrap_err();
        assert!(matches!(err, CommandError::UnknownCommand(_)));
        assert_eq!(err.to_string(), "unknown command 'nope'");
    }

    #[test]
    fn unknown_subcommand_returns_error() {
        let reg = make_registry();
        let mut session = AgentSession::new();
        let err = reg
            .execute("ping", Some("zing"), &[], &mut session)
            .unwrap_err();
        assert!(matches!(err, CommandError::UnknownSubcommand { .. }));
    }

    #[test]
    fn command_names_are_sorted() {
        let mut reg = CommandRegistry::new();
        reg.register(CommandHandler::new("z"));
        reg.register(CommandHandler::new("a"));
        reg.register(CommandHandler::new("m"));
        assert_eq!(reg.command_names(), vec!["a", "m", "z"]);
    }

    #[test]
    fn contains_known_command() {
        let reg = make_registry();
        assert!(reg.contains("ping"));
        assert!(!reg.contains("unknown"));
    }
}
