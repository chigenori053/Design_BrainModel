use crate::agent::Agent;
use crate::runtime_context::RuntimeContext;

#[derive(Default)]
pub struct ExecutionScheduler {
    agents: Vec<Box<dyn Agent>>,
}

impl ExecutionScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_agents(agents: Vec<Box<dyn Agent>>) -> Self {
        Self { agents }
    }

    pub fn register(&mut self, agent: Box<dyn Agent>) {
        self.agents.push(agent);
    }

    pub fn run(&mut self, ctx: &mut RuntimeContext) {
        for agent in &mut self.agents {
            agent.execute(ctx);
        }
        ctx.tick = ctx.tick.saturating_add(1);
    }

    pub fn len(&self) -> usize {
        self.agents.len()
    }

    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }
}
