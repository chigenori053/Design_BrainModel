use world_model_core::WorldState;

use crate::pattern_extractor::layer_sequence_from_state;
use crate::pattern_store::{DesignPattern, PatternStore};

#[derive(Clone, Debug, PartialEq)]
pub struct PatternMatch {
    pub pattern: DesignPattern,
    pub score: f64,
}

pub fn match_patterns(state: &WorldState, store: &PatternStore) -> Vec<PatternMatch> {
    let current_graph = state.architecture.causal_graph();
    let current_edges = normalized_edges(&current_graph);
    let current_layers = layer_sequence_from_state(state);
    let mut matches = store
        .patterns
        .iter()
        .filter_map(|pattern| {
            let pattern_edges = normalized_edges(&pattern.causal_graph);
            let edge_overlap = pattern_edges
                .iter()
                .filter(|edge| current_edges.contains(*edge))
                .count();
            let edge_score = if pattern_edges.is_empty() {
                0.0
            } else {
                edge_overlap as f64 / pattern_edges.len() as f64
            };

            let layer_overlap = pattern
                .layer_sequence
                .iter()
                .filter(|layer| current_layers.contains(layer))
                .count();
            let layer_score = if pattern.layer_sequence.is_empty() {
                0.0
            } else {
                layer_overlap as f64 / pattern.layer_sequence.len() as f64
            };

            let node_gap = pattern
                .causal_graph
                .nodes()
                .count()
                .abs_diff(current_graph.nodes().count());
            let size_score = 1.0 / (1.0 + node_gap as f64);
            let score = (edge_score * 0.5 + layer_score * 0.3 + size_score * 0.2)
                * pattern.average_score.max(0.1);

            (score > 0.05).then(|| PatternMatch {
                pattern: pattern.clone(),
                score,
            })
        })
        .collect::<Vec<_>>();

    matches.sort_by(|lhs, rhs| {
        rhs.score
            .total_cmp(&lhs.score)
            .then_with(|| {
                rhs.pattern
                    .average_score
                    .total_cmp(&lhs.pattern.average_score)
            })
            .then_with(|| lhs.pattern.pattern_id.cmp(&rhs.pattern.pattern_id))
    });
    matches
}

fn normalized_edges(graph: &causal_domain::CausalGraph) -> Vec<String> {
    let mut nodes = graph.nodes().copied().collect::<Vec<_>>();
    nodes.sort_unstable();
    let order = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (*node, index))
        .collect::<std::collections::HashMap<_, _>>();
    let mut edges = graph
        .edges()
        .iter()
        .map(|edge| format!("{}:{}:{:?}", order[&edge.from], order[&edge.to], edge.kind))
        .collect::<Vec<_>>();
    edges.sort();
    edges
}
