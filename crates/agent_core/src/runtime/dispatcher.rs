use crate::agent::AgentContext;
use crate::domain::{AgentOutput, AgentRequest, DomainError};
use crate::runtime::registry::AgentRegistry;

#[derive(Debug, Default, Clone, Copy)]
pub struct Dispatcher;

impl Dispatcher {
    pub fn dispatch(
        &self,
        registry: &mut AgentRegistry,
        req: AgentRequest,
        ctx: &AgentContext<'_>,
    ) -> Result<AgentOutput, DomainError> {
        let agent = registry
            .get_mut(&req.target)
            .ok_or_else(|| DomainError::AgentNotFound(req.target.clone()))?;
        agent.handle(req.input, ctx)
    }
}
