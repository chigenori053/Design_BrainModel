mod code;
mod project;

use crate::command::{CommandHandler, CommandPlugin, CommandRegistry};

pub struct AnalyzePlugin;

impl CommandPlugin for AnalyzePlugin {
    fn register(&self, registry: &mut CommandRegistry) {
        let mut cmd = CommandHandler::new("analyze");
        cmd.register_subcommand(code::handler());
        cmd.register_subcommand(project::handler());
        registry.register(cmd);
    }
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
        assert!(out.message.contains("code"));
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
            out.message.contains("Project Summary:"),
            "got: {}",
            out.message
        );
    }
}
