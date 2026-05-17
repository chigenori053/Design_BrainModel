use crate::runtime::execution_governance::{
    CommandType, ExecutionMode, ExecutionRequest, validate_execution_request,
};
use crate::runtime::shell::ResolvedExecutionTarget;

pub struct GovernanceInvariantSuite;

impl GovernanceInvariantSuite {
    pub fn assert_no_governance_bypass(
        request: &ExecutionRequest,
        expected_target: &ResolvedExecutionTarget,
    ) {
        let validation =
            validate_execution_request(request, ExecutionMode::GovernedExecute, expected_target);
        if request.command_type == CommandType::Forbidden {
            assert!(!validation.allowed);
        }
        if request.resolved_target != *expected_target {
            assert!(!validation.allowed);
        }
    }

    pub fn assert_forbidden_rejected(request: &ExecutionRequest) {
        if request.command_type == CommandType::Forbidden {
            let validation = validate_execution_request(
                request,
                ExecutionMode::GovernedExecute,
                &request.resolved_target,
            );
            assert!(!validation.allowed);
        }
    }
}
