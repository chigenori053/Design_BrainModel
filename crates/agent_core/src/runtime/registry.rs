use std::collections::HashMap;

use crate::agent::Agent;

#[derive(Default)]
pub struct AgentRegistry {
    agents: HashMap<String, Box<dyn Agent>>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, agent: Box<dyn Agent>) -> Option<Box<dyn Agent>> {
        self.agents.insert(agent.name().to_string(), agent)
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut Box<dyn Agent>> {
        self.agents.get_mut(name)
    }

    pub fn contains(&self, name: &str) -> bool {
        self.agents.contains_key(name)
    }
}
