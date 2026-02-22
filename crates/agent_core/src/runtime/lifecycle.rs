use crate::domain::AgentRequest;

pub trait AgentLifecycle {
    fn on_dispatch_start(&mut self, _request: &AgentRequest) {}
    fn on_dispatch_end(&mut self, _request: &AgentRequest) {}
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NoopLifecycle;

impl AgentLifecycle for NoopLifecycle {}
