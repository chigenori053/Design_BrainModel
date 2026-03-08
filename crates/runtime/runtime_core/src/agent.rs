use world_model_core::{ConsistencyScore, Hypothesis};

use crate::context::Phase9RuntimeContext;
use crate::ports::RuntimeResult;

#[derive(Debug, Clone, PartialEq)]
pub struct AgentInput {
    pub stage: &'static str,
    pub input_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentOutput {
    pub stage: &'static str,
    pub emitted_outputs: usize,
    pub selected_hypothesis: Option<Hypothesis>,
    pub consistency: Option<ConsistencyScore>,
}

pub trait RuntimeAgent {
    fn name(&self) -> &'static str;
    fn run(&mut self, ctx: &mut Phase9RuntimeContext) -> RuntimeResult<AgentOutput>;
}
