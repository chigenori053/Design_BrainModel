use std::collections::HashMap;

use crate::{ConceptGraph, ConceptId};

pub fn spread_activation(
    graph: &ConceptGraph,
    seeds: &[(ConceptId, f32)],
    decay: f32,
    steps: usize,
) -> HashMap<ConceptId, f32> {
    let mut scores = HashMap::new();
    for (id, score) in seeds {
        scores.insert(*id, (*score).clamp(0.0, 1.0));
    }

    for _ in 0..steps {
        let mut next = scores.clone();
        for edge in graph.edges() {
            let source_score = *scores.get(&edge.source).unwrap_or(&0.0);
            if source_score <= 0.0 {
                continue;
            }

            let propagated = (source_score * decay).clamp(0.0, 1.0);
            let entry = next.entry(edge.target).or_insert(0.0);
            *entry = (*entry + propagated).clamp(0.0, 1.0);
        }
        scores = next;
    }

    scores
}
