/// Phase1: Command Registry & Plugin System
///
/// 旧 command.rs の Command enum（loop.rs で使用）を保持しつつ、
/// Phase1 の新型（Output / CommandError / Registry / Handler / Plugin）を追加する。
pub mod handler;
pub mod plugin;
pub mod registry;

pub use handler::{CommandHandler, SubCommandHandler};
pub use plugin::CommandPlugin;
pub use registry::CommandRegistry;

// ── 旧 command.rs 互換（loop.rs が依存） ──────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Exit,
    Reset,
    None,
}

pub fn parse_command(input: &str) -> Command {
    match input.trim() {
        "/exit" | "/quit" | "exit" | "quit" => Command::Exit,
        "/reset" | "reset" => Command::Reset,
        _ => Command::None,
    }
}

// ── Phase1 型定義 ──────────────────────────────────────────────────────────

/// コマンド実行結果
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Output {
    pub message: String,
}

impl Output {
    pub fn text(s: impl Into<String>) -> Self {
        Self { message: s.into() }
    }
}

/// コマンド実行エラー
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandError {
    UnknownCommand(String),
    UnknownSubcommand { command: String, subcommand: String },
    ExecutionError(String),
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownCommand(name) => write!(f, "unknown command '{name}'"),
            Self::UnknownSubcommand {
                command,
                subcommand,
            } => {
                write!(f, "unknown subcommand '{subcommand}' for '{command}'")
            }
            Self::ExecutionError(msg) => write!(f, "execution error: {msg}"),
        }
    }
}

impl std::error::Error for CommandError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_exit_variants() {
        assert_eq!(parse_command("/exit"), Command::Exit);
        assert_eq!(parse_command("/quit"), Command::Exit);
        assert_eq!(parse_command("exit"), Command::Exit);
    }

    #[test]
    fn parse_reset() {
        assert_eq!(parse_command("/reset"), Command::Reset);
    }

    #[test]
    fn parse_none_for_unknown() {
        assert_eq!(parse_command("other"), Command::None);
    }

    #[test]
    fn output_text() {
        let o = Output::text("hello");
        assert_eq!(o.message, "hello");
    }

    #[test]
    fn command_error_display() {
        assert_eq!(
            CommandError::UnknownCommand("foo".into()).to_string(),
            "unknown command 'foo'"
        );
        assert_eq!(
            CommandError::UnknownSubcommand {
                command: "gen".into(),
                subcommand: "bar".into(),
            }
            .to_string(),
            "unknown subcommand 'bar' for 'gen'"
        );
    }
}
