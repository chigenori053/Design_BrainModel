mod design;
mod spec;

use crate::command::{CommandHandler, CommandPlugin, CommandRegistry};

pub struct GeneratePlugin;

impl CommandPlugin for GeneratePlugin {
    fn register(&self, registry: &mut CommandRegistry) {
        let mut cmd = CommandHandler::new("generate");
        cmd.register_subcommand(spec::handler());
        cmd.register_subcommand(design::handler());
        registry.register(cmd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::AgentSession;

    fn build_registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        GeneratePlugin.register(&mut registry);
        registry
    }

    #[test]
    fn generate_spec_via_registry() {
        let registry = build_registry();
        let mut session = AgentSession::new();
        let out = registry
            .execute("generate", Some("spec"), &["cli".to_string()], &mut session)
            .unwrap();
        assert!(out.message.contains("# Spec: cli"));
    }

    #[test]
    fn generate_design_via_registry() {
        let registry = build_registry();
        let mut session = AgentSession::new();
        let out = registry
            .execute("generate", Some("design"), &[], &mut session)
            .unwrap();
        assert!(out.message.contains("# Design: default"));
    }

    #[test]
    fn generate_without_subcommand_lists_available() {
        let registry = build_registry();
        let mut session = AgentSession::new();
        let out = registry
            .execute("generate", None, &[], &mut session)
            .unwrap();
        assert!(out.message.contains("spec") || out.message.contains("design"));
    }

    #[test]
    fn generate_unknown_subcommand_returns_error() {
        let registry = build_registry();
        let mut session = AgentSession::new();
        let err = registry
            .execute("generate", Some("unknown"), &[], &mut session)
            .unwrap_err();
        assert!(matches!(
            err,
            crate::command::CommandError::UnknownSubcommand { .. }
        ));
    }
}
