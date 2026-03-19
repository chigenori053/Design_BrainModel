use crate::engine::execution_engine::{DefaultExecutionEngine, ExecutionEngine};
use crate::engine::execution_plan::ExecutionPlan;
use crate::engine::execution_result::ExecutionResult;

#[derive(Clone, Debug, Default)]
pub struct StableV03ExecutionLayer {
    engine: DefaultExecutionEngine,
}

impl StableV03ExecutionLayer {
    pub fn execute(&self, plan: &ExecutionPlan) -> ExecutionResult {
        self.engine.execute(plan)
    }
}
