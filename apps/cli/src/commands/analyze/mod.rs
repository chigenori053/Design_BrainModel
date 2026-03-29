mod code;
pub(crate) mod project;

use crate::command::{CommandHandler, CommandPlugin, CommandRegistry};
use crate::session::AgentSession;
use crate::{command::CommandError, command::Output};

pub struct AnalyzePlugin;

impl CommandPlugin for AnalyzePlugin {
    fn register(&self, registry: &mut CommandRegistry) {
        let mut cmd = CommandHandler::new("analyze");
        cmd.set_default(execute_default);
        cmd.register_subcommand(code::handler());
        cmd.register_subcommand(project::handler());
        registry.register(cmd);
    }
}

fn execute_default(args: &[String], session: &mut AgentSession) -> Result<Output, CommandError> {
    if args.is_empty() {
        return Ok(Output::text(
            "Available analyze subcommands:\n- project [path]\n- code [path]",
        ));
    }
    project::execute_unified(args, session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::AgentSession;

    fn build_registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        AnalyzePlugin.register(&mut registry);
        registry
    }

    #[test]
    fn analyze_code_via_registry() {
        let registry = build_registry();
        let mut session = AgentSession::new();
        let out = registry
            .execute("analyze", Some("code"), &["src/".to_string()], &mut session)
            .unwrap();
        assert!(out.message.contains("src/"));
    }

    #[test]
    fn analyze_without_subcommand_lists_available() {
        let registry = build_registry();
        let mut session = AgentSession::new();
        let out = registry
            .execute("analyze", None, &[], &mut session)
            .unwrap();
        assert!(
            out.message.contains("Available analyze subcommands"),
            "got: {}",
            out.message
        );
        assert!(out.message.contains("project"), "got: {}", out.message);
        assert!(out.message.contains("code"), "got: {}", out.message);
    }

    #[test]
    fn analyze_project_via_registry() {
        let registry = build_registry();
        let mut session = AgentSession::new();
        let out = registry
            .execute(
                "analyze",
                Some("project"),
                &["src/".to_string()],
                &mut session,
            )
            .unwrap();
        assert!(
            out.message.contains("Project: src/"),
            "got: {}",
            out.message
        );
    }

    #[test]
    fn analyze_default_supports_detailed_report_combo() {
        let registry = build_registry();
        let mut session = AgentSession::new();
        let out = registry
            .execute(
                "analyze",
                None,
                &[
                    ".".to_string(),
                    "--detailed".to_string(),
                    "--report".to_string(),
                    "--lang".to_string(),
                    "en".to_string(),
                ],
                &mut session,
            )
            .unwrap();
        assert!(out.message.contains("[Modules]"), "got: {}", out.message);
        assert!(
            out.message.contains("=== Report ==="),
            "got: {}",
            out.message
        );
    }
}
