use crate::command::{CommandError, Output, SubCommandHandler};
use crate::session::AgentSession;

pub fn handler() -> SubCommandHandler {
    SubCommandHandler::new("spec", execute)
}

/// /generate spec [target]
///
/// 指定したターゲットの仕様書（Markdown）を生成する。
fn execute(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let target = args.first().map(|s| s.as_str()).unwrap_or("default");
    let content = format!(
        "# Spec: {target}\n\n\
         ## Overview\n\n\
         Generated specification for `{target}`.\n\n\
         ## Requirements\n\n\
         - TBD\n\n\
         ## Constraints\n\n\
         - TBD\n"
    );
    Ok(Output::text(content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::AgentSession;

    #[test]
    fn generates_spec_for_named_target() {
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&["cli".to_string()], &mut session).unwrap();
        assert!(out.message.contains("# Spec: cli"));
        assert!(out.message.contains("## Overview"));
    }

    #[test]
    fn generates_spec_with_default_target_when_no_args() {
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&[], &mut session).unwrap();
        assert!(out.message.contains("# Spec: default"));
    }
}
