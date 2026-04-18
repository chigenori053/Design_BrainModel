use crate::command::{
    CommandError, CommandHandler, CommandPlugin, CommandRegistry, Output, SubCommandHandler,
};
use crate::service::dto::RuleReport;
use crate::session::AgentSession;

pub struct RulesPlugin;

impl CommandPlugin for RulesPlugin {
    fn register(&self, registry: &mut CommandRegistry) {
        let mut cmd = CommandHandler::new("rules");
        cmd.register_subcommand(SubCommandHandler::new("list", list));
        cmd.register_subcommand(SubCommandHandler::new("inspect", inspect));
        cmd.register_subcommand(SubCommandHandler::new("validate", validate_rule));
        cmd.register_subcommand(SubCommandHandler::new("promote", promote));
        cmd.register_subcommand(SubCommandHandler::new("rollback", rollback));
        registry.register(cmd);
    }
}

/// /rules list [--lang <lang>] [--json]
fn list(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let mut cli_args = vec!["list".to_string()];
    cli_args.extend_from_slice(args);
    crate::nl_executor::run_design_command("rules", &cli_args)
        .map(Output::text)
        .map_err(CommandError::ExecutionError)
}

/// /rules inspect <rule_id> [--lang <lang>]
fn inspect(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let mut cli_args = vec!["inspect".to_string()];
    cli_args.extend_from_slice(args);
    crate::nl_executor::run_design_command("rules", &cli_args)
        .map(Output::text)
        .map_err(CommandError::ExecutionError)
}

/// /rules validate <rule_id> [--lang <lang>]
fn validate_rule(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let mut cli_args = vec!["validate".to_string()];
    cli_args.extend_from_slice(args);
    crate::nl_executor::run_design_command("rules", &cli_args)
        .map(Output::text)
        .map_err(CommandError::ExecutionError)
}

/// /rules promote [rule_id] [--validated] [--lang <lang>]
fn promote(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let mut cli_args = vec!["promote".to_string()];
    cli_args.extend_from_slice(args);
    crate::nl_executor::run_design_command("rules", &cli_args)
        .map(Output::text)
        .map_err(CommandError::ExecutionError)
}

/// /rules rollback <rule_id> [--lang <lang>]
fn rollback(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let mut cli_args = vec!["rollback".to_string()];
    cli_args.extend_from_slice(args);
    crate::nl_executor::run_design_command("rules", &cli_args)
        .map(Output::text)
        .map_err(CommandError::ExecutionError)
}

pub fn retired_rule_reports(
    store: &code_language_core::stable_v03::dynamic_ir::RuleStore,
) -> Vec<RuleReport> {
    store
        .deprecated_rules
        .iter()
        .map(|record| RuleReport {
            id: record.rule.id.clone(),
            priority: record.rule.priority,
            confidence: record.rule.confidence,
            usage_count: record.rule.usage_count,
            source: match &record.rule.source {
                code_language_core::stable_v03::dynamic_ir::RuleSource::Static => "static",
                code_language_core::stable_v03::dynamic_ir::RuleSource::Learned => "learned",
                code_language_core::stable_v03::dynamic_ir::RuleSource::User => "user",
            }
            .to_string(),
            bucket: "retired".to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_registry() -> CommandRegistry {
        let mut registry = CommandRegistry::new();
        RulesPlugin.register(&mut registry);
        registry
    }

    #[test]
    fn rules_is_registered() {
        let registry = build_registry();
        assert!(registry.command_names().contains(&"rules"));
    }

    #[test]
    fn rules_subcommands_are_available() {
        let registry = build_registry();
        let mut session = AgentSession::new();
        // subcommand なしで subcommand 一覧が返る
        let out = registry.execute("rules", None, &[], &mut session).unwrap();
        assert!(
            out.message.contains("list")
                || out.message.contains("inspect")
                || out.message.contains("Available"),
        );
    }
}
