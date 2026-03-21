use crate::command::{CommandError, Output, SubCommandHandler};
use crate::session::AgentSession;

pub fn handler() -> SubCommandHandler {
    SubCommandHandler::new("design", execute)
}

/// /generate design [target]
///
/// 指定したターゲットの設計ドキュメント（Markdown）を生成する。
fn execute(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let target = args.first().map(|s| s.as_str()).unwrap_or("default");
    let content = format!(
        "# Design: {target}\n\n\
         ## Architecture\n\n\
         Generated design document for `{target}`.\n\n\
         ## Components\n\n\
         - TBD\n\n\
         ## Data Flow\n\n\
         - TBD\n"
    );
    Ok(Output::text(content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::AgentSession;

    #[test]
    fn generates_design_for_named_target() {
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&["api".to_string()], &mut session).unwrap();
        assert!(out.message.contains("# Design: api"));
        assert!(out.message.contains("## Architecture"));
    }

    #[test]
    fn generates_design_with_default_target_when_no_args() {
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&[], &mut session).unwrap();
        assert!(out.message.contains("# Design: default"));
    }
}
