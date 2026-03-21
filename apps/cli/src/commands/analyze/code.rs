use crate::command::{CommandError, Output, SubCommandHandler};
use crate::session::AgentSession;

pub fn handler() -> SubCommandHandler {
    SubCommandHandler::new("code", execute)
}

/// /analyze code [path]
///
/// 指定パスのコードを解析してサマリを出力する。
fn execute(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let path = args.first().map(|s| s.as_str()).unwrap_or(".");
    Ok(Output::text(format!(
        "Analyzing code at: {path}\n\
         (stub) No issues found."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::AgentSession;

    #[test]
    fn analyze_code_with_path() {
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&["src/".to_string()], &mut session).unwrap();
        assert!(out.message.contains("src/"));
    }

    #[test]
    fn analyze_code_defaults_to_current_dir() {
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&[], &mut session).unwrap();
        assert!(out.message.contains("Analyzing code at: ."));
    }
}
