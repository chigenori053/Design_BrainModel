pub mod design_agent;
pub mod learning_agent;
pub mod search_agent;
pub mod websearch_agent;

use crate::domain::{AgentInput, AgentOutput, DomainError};
use crate::ports::{MemoryPort, TelemetryPort};

pub struct AgentContext<'a> {
    pub memory: &'a dyn MemoryPort,
    pub telemetry: &'a dyn TelemetryPort,
}

pub trait Agent: Send {
    fn name(&self) -> &'static str;

    fn handle(
        &mut self,
        input: AgentInput,
        ctx: &AgentContext<'_>,
    ) -> Result<AgentOutput, DomainError>;
}

pub use design_agent::DesignAgent;
pub use learning_agent::LearningAgent;
pub use search_agent::SearchAgent;
pub use websearch_agent::WebSearchAgent;
