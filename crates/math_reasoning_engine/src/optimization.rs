use crate::{ComplexityEstimate, GraphMetrics};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OptimizationResult {
    pub score: f64,
    pub pareto_rank: usize,
}

pub trait OptimizationEngine {
    fn optimize(
        &self,
        complexity: ComplexityEstimate,
        graph: GraphMetrics,
        constraint_satisfied: bool,
    ) -> OptimizationResult;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DeterministicOptimizationEngine;

impl OptimizationEngine for DeterministicOptimizationEngine {
    fn optimize(
        &self,
        complexity: ComplexityEstimate,
        graph: GraphMetrics,
        constraint_satisfied: bool,
    ) -> OptimizationResult {
        let complexity_score = complexity.score();
        let graph_penalty = ((graph.cycle_count as f64 * 0.2)
            + (graph.max_fan_out as f64 * 0.05)
            + (graph.max_depth.saturating_sub(1) as f64 * 0.03))
            .clamp(0.0, 0.9);
        let score = (complexity_score * 0.45
            + (1.0 - graph_penalty) * 0.35
            + if constraint_satisfied { 0.20 } else { 0.05 })
            .clamp(0.0, 1.0);
        let pareto_rank = if score >= 0.85 {
            0
        } else if score >= 0.65 {
            1
        } else if score >= 0.45 {
            2
        } else {
            3
        };

        OptimizationResult { score, pareto_rank }
    }
}
