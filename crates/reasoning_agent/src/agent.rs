use memory_space_api::{MemoryEngine, MemoryQuery, ScoredCandidate};
use memory_space_complex::ComplexField;
use memory_space_index::MemoryIndex;

use crate::evaluation::evaluate;
use crate::hypothesis::generate_hypotheses_with_context;
use crate::perception::perceive;
use crate::simulator::simulate;
use crate::types::{ReasoningInput, ReasoningResult, ReasoningStats};

pub struct ReasoningAgent<M: MemoryIndex> {
    memory: MemoryEngine<M>,
    recall_threshold: f64,
    top_k: usize,
    max_hypotheses: usize,
    max_depth: usize,
    entropy_threshold: f64,
}

impl<M: MemoryIndex> ReasoningAgent<M> {
    pub fn new(memory: MemoryEngine<M>) -> Self {
        Self {
            memory,
            recall_threshold: 0.7,
            top_k: 3,
            max_hypotheses: 8,
            max_depth: 2,
            entropy_threshold: 0.45,
        }
    }

    pub fn with_config(
        memory: MemoryEngine<M>,
        recall_threshold: f64,
        top_k: usize,
        max_hypotheses: usize,
        max_depth: usize,
        entropy_threshold: f64,
    ) -> Self {
        Self {
            memory,
            recall_threshold: recall_threshold.clamp(0.0, 1.0),
            top_k: top_k.max(1),
            max_hypotheses: max_hypotheses.max(1),
            max_depth: max_depth.clamp(1, 3),
            entropy_threshold: entropy_threshold.max(0.0),
        }
    }

    pub fn reason(&self, input: ReasoningInput) -> ReasoningResult {
        let state = perceive(&input.semantic_vector);
        let query = MemoryQuery {
            vector: state.clone(),
            context: input.context.clone(),
            k: self.top_k,
        };

        let recalled = self.memory.query(query);
        let recall_entropy = resonance_entropy(&recalled);
        let best_resonance = recalled.first().map(|c| c.resonance).unwrap_or(0.0);

        if let Some(best) = recalled.first()
            && best.resonance >= self.recall_threshold
            && recall_entropy <= self.entropy_threshold
            && let Some(solution) = self.recalled_solution(best.memory_id)
        {
            return ReasoningResult {
                solution_vector: solution,
                confidence: best.confidence,
                stats: ReasoningStats {
                    used_recall: true,
                    recall_resonance: best.resonance,
                    recall_entropy,
                    hypotheses_generated: 0,
                    simulation_steps: 0,
                    evaluation_score: best.score,
                },
            };
        }

        let hypotheses =
            generate_hypotheses_with_context(&state, input.context.as_ref(), self.max_hypotheses);

        let mut simulation_steps = 0usize;
        let best = hypotheses
            .iter()
            .enumerate()
            .map(|(idx, hypothesis)| {
                let mut current_state = simulate(&state, hypothesis);
                let mut cumulative_score = 0.0;
                for _ in 0..self.max_depth {
                    cumulative_score += evaluate(&current_state, &self.memory, self.top_k);
                    simulation_steps += 1;
                    current_state = simulate(&current_state, hypothesis);
                }
                let mean_score = cumulative_score / self.max_depth as f64;
                (idx, mean_score, current_state)
            })
            .max_by(|lhs, rhs| lhs.1.total_cmp(&rhs.1).then_with(|| rhs.0.cmp(&lhs.0)));

        if let Some((_, score, state)) = best {
            return ReasoningResult {
                solution_vector: state,
                confidence: score.clamp(0.0, 1.0),
                stats: ReasoningStats {
                    used_recall: false,
                    recall_resonance: best_resonance,
                    recall_entropy,
                    hypotheses_generated: hypotheses.len(),
                    simulation_steps,
                    evaluation_score: score,
                },
            };
        }

        ReasoningResult {
            solution_vector: fallback_solution(&state, input.context.as_ref()),
            confidence: 0.0,
            stats: ReasoningStats {
                used_recall: false,
                recall_resonance: best_resonance,
                recall_entropy,
                hypotheses_generated: 0,
                simulation_steps: 0,
                evaluation_score: 0.0,
            },
        }
    }

    fn recalled_solution(&self, memory_id: u64) -> Option<ComplexField> {
        self.memory
            .memory_bank
            .iter()
            .find(|memory| memory.id == memory_id)
            .map(|memory| ComplexField::new(memory.vector.clone()))
    }
}

fn fallback_solution(semantic: &ComplexField, context: Option<&ComplexField>) -> ComplexField {
    match context {
        Some(ctx) => {
            let len = semantic.data.len().min(ctx.data.len());
            let merged = (0..len)
                .map(|idx| semantic.data[idx] + ctx.data[idx])
                .collect::<Vec<_>>();
            ComplexField::new(merged)
        }
        None => semantic.clone(),
    }
}

fn resonance_entropy(candidates: &[ScoredCandidate]) -> f64 {
    if candidates.is_empty() {
        return 0.0;
    }

    let sum = candidates
        .iter()
        .map(|candidate| candidate.resonance.max(0.0))
        .sum::<f64>();
    if sum <= f64::EPSILON {
        return 0.0;
    }

    let mut entropy = 0.0;
    for candidate in candidates {
        let p = candidate.resonance.max(0.0) / sum;
        if p > 0.0 {
            entropy -= p * p.ln();
        }
    }
    entropy
}

#[cfg(test)]
mod tests {
    use memory_space_api::MemoryEngine;
    use memory_space_complex::{encode_real_vector, normalize};
    use memory_space_core::{MemoryField, MemoryId};
    use memory_space_index::LinearIndex;

    use crate::{ReasoningAgent, types::ReasoningInput};

    fn mem(id: MemoryId, values: &[f64]) -> MemoryField {
        let mut field = encode_real_vector(values);
        normalize(&mut field);
        MemoryField {
            id,
            vector: field.data,
        }
    }

    #[test]
    fn strong_recall_overrides_search() {
        let recalled = mem(5, &[1.0, 0.0]);
        let engine = MemoryEngine::with_memory(vec![recalled.clone()], LinearIndex::new());
        let agent = ReasoningAgent::with_config(engine, 0.5, 1, 8, 2, 0.5);

        let mut semantic = encode_real_vector(&[1.0, 0.0]);
        normalize(&mut semantic);

        let out = agent.reason(ReasoningInput {
            semantic_vector: semantic,
            context: None,
        });

        assert!(out.stats.used_recall);
        assert_eq!(out.solution_vector.data, recalled.vector);
    }
}
