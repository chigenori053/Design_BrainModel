use core_types::ObjectiveVector;

use crate::domain::{Hypothesis, Score};

pub trait ScoringCapability: Send + Sync {
    fn score(&self, hypothesis: &Hypothesis) -> Score;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct LinearObjectiveScorer;

impl LinearObjectiveScorer {
    pub fn score_objective(&self, obj: &ObjectiveVector) -> f64 {
        0.4 * obj.f_struct + 0.2 * obj.f_field + 0.2 * obj.f_risk + 0.2 * obj.f_shape
    }
}
