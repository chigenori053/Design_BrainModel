use crate::agent::{Agent, AgentContext};
use crate::domain::{AgentEvent, AgentInput, AgentOutput, DomainError, TelemetryEvent};

#[derive(Debug, Default)]
pub struct LearningAgent;

impl Agent for LearningAgent {
    fn name(&self) -> &'static str {
        "learning"
    }

    fn handle(
        &mut self,
        input: AgentInput,
        _ctx: &AgentContext<'_>,
    ) -> Result<AgentOutput, DomainError> {
        if input.text.trim().is_empty() {
            return Err(DomainError::InvalidInput("learning payload is empty".to_string()));
        }

        let key = format!("learning/{}", uuid_like_key(&input.text));
        Ok(AgentOutput {
            summary: "learning update accepted".to_string(),
            artifacts: vec![key.clone()],
            events: vec![
                AgentEvent::PersistMemory {
                    key,
                    value: input.text.into_bytes(),
                },
                AgentEvent::EmitTelemetry(TelemetryEvent {
                    name: "learning.persist".to_string(),
                    value: "1".to_string(),
                }),
            ],
        })
    }
}

fn uuid_like_key(seed: &str) -> String {
    let mut h: u64 = 1469598103934665603;
    for b in seed.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    format!("{h:016x}")
}
