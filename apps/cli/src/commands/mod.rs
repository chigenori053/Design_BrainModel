pub mod analyze;
pub mod generate;
pub mod system;

use crate::command::CommandRegistry;

/// デフォルトPluginをすべて Registry に登録する
///
/// REPL 起動時にこれを呼ぶことで、全コマンドが利用可能になる。
pub fn register_defaults(registry: &mut CommandRegistry) {
    generate::GeneratePlugin.register(registry);
    analyze::AnalyzePlugin.register(registry);
    system::SystemPlugin.register(registry);
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
