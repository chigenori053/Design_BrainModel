use crate::agent::{Agent, AgentContext};
use crate::capability::{ScoringCapability, SearchCapability, rank_hits_with_scorer};
use crate::domain::{AgentEvent, AgentInput, AgentOutput, DomainError, TelemetryEvent};

pub struct SearchAgent<C: SearchCapability, S: ScoringCapability> {
    search: C,
    scorer: S,
}

impl<C: SearchCapability, S: ScoringCapability> SearchAgent<C, S> {
    pub fn new(search: C, scorer: S) -> Self {
        Self { search, scorer }
    }
}

impl<C: SearchCapability, S: ScoringCapability> Agent for SearchAgent<C, S> {
    fn name(&self) -> &'static str {
        "search"
    }

    fn handle(
        &mut self,
        input: AgentInput,
        _ctx: &AgentContext<'_>,
    ) -> Result<AgentOutput, DomainError> {
        if input.text.trim().is_empty() {
            return Err(DomainError::InvalidInput("query is empty".to_string()));
        }

        let hits = self.search.search(&input.text)?;
        let scored = rank_hits_with_scorer(&hits, &self.scorer);

        let artifacts = scored
            .into_iter()
            .take(5).map(|(hit, score)| format!("{score:.3}: {}", hit.title))
            .collect::<Vec<_>>();

        let persist_payload = artifacts.join("\n").into_bytes();
        Ok(AgentOutput {
            summary: format!("search completed for query: {}", input.text),
            artifacts,
            events: vec![
                AgentEvent::PersistMemory {
                    key: format!("search/{}", input.text),
                    value: persist_payload,
                },
                AgentEvent::EmitTelemetry(TelemetryEvent {
                    name: "search.completed".to_string(),
                    value: "1".to_string(),
                }),
            ],
        })
    }
}
