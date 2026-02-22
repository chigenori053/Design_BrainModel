use crate::agent::{Agent, AgentContext};
use crate::capability::SearchHit;
use crate::domain::{AgentInput, AgentOutput, DomainError};
use crate::ports::SearchPort;

pub struct WebSearchAgent<P: SearchPort> {
    port: P,
}

impl<P: SearchPort> WebSearchAgent<P> {
    pub fn new(port: P) -> Self {
        Self { port }
    }
}

impl<P: SearchPort> Agent for WebSearchAgent<P> {
    fn name(&self) -> &'static str {
        "websearch"
    }

    fn handle(
        &mut self,
        input: AgentInput,
        _ctx: &AgentContext<'_>,
    ) -> Result<AgentOutput, DomainError> {
        let hits = self.port.search(&input.text)?;
        Ok(AgentOutput {
            summary: format!("web search returned {} hits", hits.len()),
            artifacts: hits_to_artifacts(&hits),
            events: Vec::new(),
        })
    }
}

fn hits_to_artifacts(hits: &[SearchHit]) -> Vec<String> {
    hits.iter()
        .map(|h| format!("{} - {}", h.title, h.snippet))
        .collect()
}
