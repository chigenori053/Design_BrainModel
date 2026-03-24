pub mod analyze;
pub mod coding;
pub mod exec;
pub mod generate;
pub mod memory;
pub mod refactor;
pub mod rules;
pub mod system;
pub mod validate;

use crate::command::CommandRegistry;

/// デフォルトPluginをすべて Registry に登録する
///
/// REPL 起動時にこれを呼ぶことで、全コマンドが利用可能になる。
pub fn register_defaults(registry: &mut CommandRegistry) {
    generate::GeneratePlugin.register(registry);
    analyze::AnalyzePlugin.register(registry);
    system::SystemPlugin.register(registry);
    validate::ValidatePlugin.register(registry);
    refactor::RefactorPlugin.register(registry);
    coding::CodingPlugin.register(registry);
    exec::ExecPlugin.register(registry);
    rules::RulesPlugin.register(registry);
    memory::MemoryPlugin.register(registry);
}

use crate::command::CommandPlugin;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::AgentSession;

    #[test]
    fn defaults_register_all_commands() {
        let mut registry = CommandRegistry::new();
        register_defaults(&mut registry);
        let names = registry.command_names();
        assert!(names.contains(&"generate"), "generate should be registered");
        assert!(names.contains(&"analyze"), "analyze should be registered");
        assert!(names.contains(&"system"), "system should be registered");
        assert!(names.contains(&"validate"), "validate should be registered");
        assert!(names.contains(&"refactor"), "refactor should be registered");
        assert!(names.contains(&"coding"), "coding should be registered");
        assert!(names.contains(&"diff"), "diff should be registered");
        assert!(names.contains(&"check"), "check should be registered");
        assert!(names.contains(&"apply"), "apply should be registered");
        assert!(names.contains(&"run"), "run should be registered");
        assert!(names.contains(&"rules"), "rules should be registered");
        assert!(names.contains(&"memory"), "memory should be registered");
    }

    #[test]
    fn generate_spec_end_to_end() {
        let mut registry = CommandRegistry::new();
        register_defaults(&mut registry);
        let mut session = AgentSession::new();
        let out = registry
            .execute("generate", Some("spec"), &["cli".to_string()], &mut session)
            .unwrap();
        assert!(out.message.contains("# Spec: cli"));
    }

    #[test]
    fn analyze_code_end_to_end() {
        let mut registry = CommandRegistry::new();
        register_defaults(&mut registry);
        let mut session = AgentSession::new();
        let out = registry
            .execute("analyze", Some("code"), &[], &mut session)
            .unwrap();
        assert!(out.message.contains("Analyzing code"));
    }

    #[test]
    fn system_status_end_to_end() {
        let mut registry = CommandRegistry::new();
        register_defaults(&mut registry);
        let mut session = AgentSession::new();
        let out = registry
            .execute("system", Some("status"), &[], &mut session)
            .unwrap();
        assert!(out.message.contains("idle"));
    }
}
