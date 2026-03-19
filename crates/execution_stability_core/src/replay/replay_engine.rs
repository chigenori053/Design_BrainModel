use crate::controller::execution_controller::{
    DefaultExecutionController, ExecutionController, ExecutionResult,
};
use crate::reproducibility::snapshot::ExecutionSnapshot;
use execution_core::engine::execution_plan::ExecutionPlan;

pub trait ReplayEngine: Send + Sync {
    fn replay(&self, snapshot: &ExecutionSnapshot, plan: &ExecutionPlan) -> ExecutionResult;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultReplayEngine;

impl ReplayEngine for DefaultReplayEngine {
    fn replay(&self, snapshot: &ExecutionSnapshot, plan: &ExecutionPlan) -> ExecutionResult {
        let result = DefaultExecutionController::default().execute_with_control(plan);
        if result.snapshot == *snapshot {
            result
        } else {
            let mut replayed = result;
            replayed.failure_type = replayed.failure_type.or(Some(
                crate::failure::failure_type::FailureType::EnvironmentError,
            ));
            replayed
        }
    }
}
