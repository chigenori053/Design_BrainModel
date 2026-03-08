pub mod propagation;
pub mod scoring;

use std::collections::HashMap;

use crate::{ConceptGraph, ConceptId};

pub use propagation::spread_activation;
pub use scoring::top_k_activation;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActivationEngine {
    pub propagation_steps: usize,
    pub decay: f64,
}

impl Default for ActivationEngine {
    fn default() -> Self {
        Self {
            propagation_steps: 2,
            decay: 0.7,
        }
    }
}

impl ActivationEngine {
    pub fn run(
        &self,
        graph: &ConceptGraph,
        intent_concepts: &[ConceptId],
    ) -> HashMap<ConceptId, f32> {
        let seeds = intent_concepts
            .iter()
            .copied()
            .map(|id| (id, 1.0))
            .collect::<Vec<_>>();

        spread_activation(
            graph,
            &seeds,
            self.decay.clamp(0.0, 1.0) as f32,
            self.propagation_steps,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::{ConceptEdge, ConceptGraph, ConceptId, RelationType};

    use super::{ActivationEngine, spread_activation, top_k_activation};

    #[test]
    fn activation_spreads_over_edges() {
        let a = ConceptId::from_name("A");
        let b = ConceptId::from_name("B");

        let mut graph = ConceptGraph::default();
        graph.add_edge(ConceptEdge {
            source: a,
            relation: RelationType::DependsOn,
            target: b,
        });

        let scores = spread_activation(&graph, &[(a, 1.0)], 0.5, 1);
        assert!(scores.get(&b).copied().unwrap_or(0.0) > 0.0);

        let ranked = top_k_activation(&scores, 1);
        assert_eq!(ranked.len(), 1);
    }

    #[test]
    fn activation_engine_uses_intent_seed() {
        let a = ConceptId::from_name("A");
        let b = ConceptId::from_name("B");
        let mut graph = ConceptGraph::default();
        graph.add_edge(ConceptEdge {
            source: a,
            relation: RelationType::DependsOn,
            target: b,
        });

        let engine = ActivationEngine {
            propagation_steps: 1,
            decay: 0.5,
        };
        let scores = engine.run(&graph, &[a]);
        assert!(scores.get(&a).copied().unwrap_or(0.0) > 0.0);
        assert!(scores.get(&b).copied().unwrap_or(0.0) > 0.0);
    }
}
