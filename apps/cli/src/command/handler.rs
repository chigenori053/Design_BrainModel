use std::collections::HashMap;

use crate::session::AgentSession;

use super::{CommandError, Output};

type CommandFn = fn(&[String], &mut AgentSession) -> Result<Output, CommandError>;

/// 実際の処理を担う最小単位
///
/// 関数ポインタを使うことで軽量かつテスト容易性を確保する。
pub struct SubCommandHandler {
    pub name: String,
    pub execute: CommandFn,
}

impl SubCommandHandler {
    pub fn new(name: &str, execute: CommandFn) -> Self {
        Self {
            name: name.to_string(),
            execute,
        }
    }
}

/// Command 単位のハンドラ（SubCommand を管理する）
pub struct CommandHandler {
    pub name: String,
    subcommands: HashMap<String, SubCommandHandler>,
    default: Option<CommandFn>,
}

impl CommandHandler {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            subcommands: HashMap::new(),
            default: None,
        }
    }

    /// SubCommand を登録する
    pub fn register_subcommand(&mut self, sub: SubCommandHandler) {
        self.subcommands.insert(sub.name.clone(), sub);
    }

    /// subcommand なしで呼ばれた場合のデフォルト処理を設定する
    pub fn set_default(
        &mut self,
        f: fn(&[String], &mut AgentSession) -> Result<Output, CommandError>,
    ) {
        self.default = Some(f);
    }

    /// subcommand を分岐して実行する
    pub fn execute(
        &self,
        subcommand: Option<&str>,
        args: &[String],
        session: &mut AgentSession,
    ) -> Result<Output, CommandError> {
        match subcommand {
            Some(sub) => {
                if let Some(handler) = self.subcommands.get(sub) {
                    return (handler.execute)(args, session);
                }
                if let Some(default) = self.default {
                    let mut forwarded = vec![sub.to_string()];
                    forwarded.extend_from_slice(args);
                    return default(&forwarded, session);
                }
                Err(CommandError::UnknownSubcommand {
                    command: self.name.clone(),
                    subcommand: sub.to_string(),
                })
            }
            None => {
                if let Some(default) = self.default {
                    return default(args, session);
                }
                let mut subs: Vec<&str> = self.subcommands.keys().map(|s| s.as_str()).collect();
                subs.sort();
                Ok(Output::text(format!(
                    "Command '{}' requires a subcommand. Available: {}",
                    self.name,
                    subs.join(", ")
                )))
            }
        }
    }

    /// 登録済みサブコマンド名の一覧（ソート済み）
    pub fn subcommand_names(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.subcommands.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::AgentSession;

    fn echo_handler(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
        Ok(Output::text(args.join(" ")))
    }

    fn build_cmd() -> CommandHandler {
        let mut cmd = CommandHandler::new("test");
        cmd.register_subcommand(SubCommandHandler::new("echo", echo_handler));
        cmd
    }

    #[test]
    fn execute_known_subcommand() {
        let cmd = build_cmd();
        let mut session = AgentSession::new();
        let out = cmd
            .execute(Some("echo"), &["hello".to_string()], &mut session)
            .unwrap();
        assert_eq!(out.message, "hello");
    }

    #[test]
    fn execute_unknown_subcommand_returns_error() {
        let cmd = build_cmd();
        let mut session = AgentSession::new();
        let err = cmd.execute(Some("nope"), &[], &mut session).unwrap_err();
        assert!(matches!(err, CommandError::UnknownSubcommand { .. }));
    }

    #[test]
    fn default_command_accepts_subcommand_as_first_arg() {
        let mut cmd = CommandHandler::new("apply");
        cmd.set_default(echo_handler);
        let mut session = AgentSession::new();
        let out = cmd
            .execute(Some("."), &["--json".to_string()], &mut session)
            .unwrap();
        assert_eq!(out.message, ". --json");
    }

    #[test]
    fn execute_no_subcommand_lists_available() {
        let cmd = build_cmd();
        let mut session = AgentSession::new();
        let out = cmd.execute(None, &[], &mut session).unwrap();
        assert!(out.message.contains("echo"));
    }

    #[test]
    fn subcommand_names_are_sorted() {
        let mut cmd = CommandHandler::new("x");
        cmd.register_subcommand(SubCommandHandler::new("z", echo_handler));
        cmd.register_subcommand(SubCommandHandler::new("a", echo_handler));
        cmd.register_subcommand(SubCommandHandler::new("m", echo_handler));
        let names = cmd.subcommand_names();
        assert_eq!(names, vec!["a", "m", "z"]);
    }
}
