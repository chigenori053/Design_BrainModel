use architecture_ir::{ArchitectureAnalyzer, ArchitectureIR, BasicArchitectureAnalyzer};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ArchitectureScore {
    pub coupling: f32,
    pub cohesion: f32,
    pub complexity: f32,
    pub layering_score: f32,
}

impl ArchitectureScore {
    pub fn desirability(&self) -> f32 {
        cohesion_gain(self.cohesion)
            + layering_gain(self.layering_score)
            + inverse_cost(self.coupling)
            + inverse_cost(self.complexity)
    }
}

pub trait ArchitectureEvaluator {
    fn evaluate(&self, ir: &ArchitectureIR) -> ArchitectureScore;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BasicArchitectureEvaluator;

impl ArchitectureEvaluator for BasicArchitectureEvaluator {
    fn evaluate(&self, ir: &ArchitectureIR) -> ArchitectureScore {
        let result = BasicArchitectureAnalyzer.analyze(ir);
        ArchitectureScore {
            coupling: result.metrics.coupling,
            cohesion: result.metrics.cohesion,
            complexity: result.metrics.complexity_score,
            layering_score: result.metrics.layering_score,
        }
    }
}

pub fn score_dominates(left: &ArchitectureScore, right: &ArchitectureScore) -> bool {
    let no_worse = left.coupling <= right.coupling
        && left.complexity <= right.complexity
        && left.cohesion >= right.cohesion
        && left.layering_score >= right.layering_score;
    let strictly_better = left.coupling < right.coupling
        || left.complexity < right.complexity
        || left.cohesion > right.cohesion
        || left.layering_score > right.layering_score;
    no_worse && strictly_better
}

fn inverse_cost(value: f32) -> f32 {
    1.0 / (1.0 + value.max(0.0))
}

fn cohesion_gain(value: f32) -> f32 {
    value.max(0.0)
}

fn layering_gain(value: f32) -> f32 {
    value.max(0.0)
}
