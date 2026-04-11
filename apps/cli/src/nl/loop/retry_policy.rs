use super::promotion::LoopOrigin;
use super::state::{EscalationReason, FailureClass, RetryPolicy};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetryBudget {
    pub max_attempts: u8,
    pub confidence_floor: f32,
    pub no_op_limit: u8,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConfidencePolicy {
    pub promote_threshold: f32,
    pub continue_threshold: f32,
    pub retry_decay: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RetryDecision {
    Rollback,
    Escalate(EscalationReason),
    ChangeStrategy,
    Replan,
}

pub struct RetryEvaluator;

impl RetryEvaluator {
    pub fn budget_for_origin(origin: LoopOrigin) -> RetryBudget {
        match origin {
            LoopOrigin::Analyze => RetryBudget {
                max_attempts: 3,
                confidence_floor: 0.55,
                no_op_limit: 1,
            },
            LoopOrigin::Coding => RetryBudget {
                max_attempts: 2,
                confidence_floor: 0.70,
                no_op_limit: 1,
            },
            LoopOrigin::Validate => RetryBudget {
                max_attempts: 2,
                confidence_floor: 0.70,
                no_op_limit: 1,
            },
            LoopOrigin::Structure => RetryBudget {
                max_attempts: 2,
                confidence_floor: 0.70,
                no_op_limit: 1,
            },
            LoopOrigin::MemoryRecall => RetryBudget {
                max_attempts: 1,
                confidence_floor: 0.80,
                no_op_limit: 0,
            },
            LoopOrigin::PreviousTransaction => RetryBudget {
                max_attempts: 1,
                confidence_floor: 0.70,
                no_op_limit: 0,
            },
            LoopOrigin::SubcommandBridge => RetryBudget {
                max_attempts: 1,
                confidence_floor: 0.70,
                no_op_limit: 0,
            },
        }
    }

    pub fn confidence_policy_for_origin(origin: LoopOrigin) -> ConfidencePolicy {
        match origin {
            LoopOrigin::Analyze => ConfidencePolicy {
                promote_threshold: 0.55,
                continue_threshold: 0.55,
                retry_decay: 0.10,
            },
            LoopOrigin::Structure => ConfidencePolicy {
                promote_threshold: 0.70,
                continue_threshold: 0.70,
                retry_decay: 0.10,
            },
            LoopOrigin::MemoryRecall => ConfidencePolicy {
                promote_threshold: 0.80,
                continue_threshold: 0.80,
                retry_decay: 0.05,
            },
            LoopOrigin::Coding | LoopOrigin::Validate => ConfidencePolicy {
                promote_threshold: 0.70,
                continue_threshold: 0.70,
                retry_decay: 0.10,
            },
            LoopOrigin::PreviousTransaction | LoopOrigin::SubcommandBridge => ConfidencePolicy {
                promote_threshold: 0.70,
                continue_threshold: 0.70,
                retry_decay: 0.10,
            },
        }
    }

    pub fn retry_policy_for_origin(origin: LoopOrigin) -> RetryPolicy {
        let budget = Self::budget_for_origin(origin);
        RetryPolicy {
            max_attempts: budget.max_attempts,
            confidence_floor_milli: (budget.confidence_floor * 1000.0) as u16,
            no_op_limit: budget.no_op_limit,
        }
    }

    pub fn decide(
        policy: RetryPolicy,
        attempts: u8,
        confidence: f32,
        no_op_count: u8,
        current_failure: FailureClass,
        previous_failure: Option<FailureClass>,
        improvement_detected: bool,
    ) -> RetryDecision {
        if attempts >= policy.max_attempts {
            return RetryDecision::Rollback;
        }
        if confidence < policy.confidence_floor() {
            return RetryDecision::Escalate(EscalationReason::ConfidenceCollapsed);
        }
        if no_op_count >= policy.no_op_limit || current_failure == FailureClass::NoImprovement {
            return RetryDecision::Rollback;
        }
        if previous_failure == Some(current_failure) {
            return RetryDecision::ChangeStrategy;
        }
        if improvement_detected {
            return RetryDecision::Replan;
        }

        RetryDecision::ChangeStrategy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_budget_stops_deterministically() {
        let decision = RetryEvaluator::decide(
            RetryPolicy::default(),
            2,
            0.9,
            0,
            FailureClass::CompileError,
            None,
            false,
        );
        assert_eq!(decision, RetryDecision::Rollback);
    }

    #[test]
    fn analyze_origin_gets_three_attempts() {
        assert_eq!(
            RetryEvaluator::budget_for_origin(LoopOrigin::Analyze).max_attempts,
            3
        );
    }

    #[test]
    fn memory_origin_gets_one_attempt() {
        assert_eq!(
            RetryEvaluator::budget_for_origin(LoopOrigin::MemoryRecall).max_attempts,
            1
        );
    }

    #[test]
    fn coding_origin_gets_two_attempts() {
        assert_eq!(
            RetryEvaluator::budget_for_origin(LoopOrigin::Coding).max_attempts,
            2
        );
    }
}
