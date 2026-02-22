use crate::agent::{Agent, AgentContext};
use crate::domain::{AgentEvent, AgentInput, AgentOutput, DomainError};

#[derive(Debug, Default)]
pub struct DesignAgent;

impl Agent for DesignAgent {
    fn name(&self) -> &'static str {
        "design"
    }

    fn handle(
        &mut self,
        input: AgentInput,
        _ctx: &AgentContext<'_>,
    ) -> Result<AgentOutput, DomainError> {
        if input.text.trim().is_empty() {
            return Err(DomainError::InvalidInput("design prompt is empty".to_string()));
        }

        Ok(AgentOutput {
            summary: "design analysis requires external knowledge".to_string(),
            artifacts: Vec::new(),
            events: vec![AgentEvent::RequestSearch { query: input.text }],
        })
    }
}
