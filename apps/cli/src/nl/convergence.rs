use super::goal::GoalType;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ConvergenceMetrics {
    pub before: f32,
    pub after: f32,
    pub confidence: f32,
    pub validation_ok: bool,
}

impl ConvergenceMetrics {
    pub fn progress_ratio(self) -> f32 {
        if self.before <= 0.0 {
            return 1.0;
        }
        ((self.before - self.after) / self.before).clamp(0.0, 1.0)
    }
}

pub fn goal_reached(goal: GoalType, metrics: ConvergenceMetrics, threshold: f32) -> bool {
    if !metrics.validation_ok || metrics.confidence <= 0.0 {
        return false;
    }
    match goal {
        GoalType::EliminateCycles => metrics.after <= 0.0,
        GoalType::ReduceUnsafe => metrics.after < metrics.before,
        GoalType::StabilizeViewerDispatch => metrics.after <= 0.0,
        GoalType::ImproveTestPassRate => metrics.progress_ratio() >= threshold,
        GoalType::PrepareCommitAndPR => metrics.progress_ratio() >= threshold,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nl::goal::GoalType;

    #[test]
    fn convergence_stops_when_cycles_zero() {
        assert!(goal_reached(
            GoalType::EliminateCycles,
            ConvergenceMetrics {
                before: 1.0,
                after: 0.0,
                confidence: 1.0,
                validation_ok: true,
            },
            0.95
        ));
    }

    #[test]
    fn convergence_rejects_validation_regression() {
        assert!(!goal_reached(
            GoalType::ReduceUnsafe,
            ConvergenceMetrics {
                before: 10.0,
                after: 5.0,
                confidence: 1.0,
                validation_ok: false,
            },
            0.95
        ));
    }
}
