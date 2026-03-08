use std::collections::BTreeMap;

use crate::agent::Agent;

pub type AgentFactory = fn() -> Box<dyn Agent>;

#[derive(Default)]
pub struct AgentRegistry {
    factories: BTreeMap<String, AgentFactory>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, name: impl Into<String>, factory: AgentFactory) {
        self.factories.insert(name.into(), factory);
    }

    pub fn build(&self, name: &str) -> Option<Box<dyn Agent>> {
        self.factories.get(name).map(|factory| factory())
    }

    pub fn names(&self) -> Vec<String> {
        self.factories.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::agent::EvaluationAgent;

    use super::AgentRegistry;

    #[test]
    fn registry_register_and_build() {
        let mut registry = AgentRegistry::new();
        registry.register("evaluation", || Box::new(EvaluationAgent));

        assert!(registry.build("evaluation").is_some());
        assert_eq!(registry.names(), vec!["evaluation".to_string()]);
    }
}
