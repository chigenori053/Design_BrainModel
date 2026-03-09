use crate::reasoning_state::ReasoningState;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReasoningScore {
    pub semantic_coherence: f64,
    pub design_alignment: f64,
    pub inference_depth: f64,
}

impl ReasoningScore {
    pub fn total(self) -> f64 {
        self.semantic_coherence + self.design_alignment + self.inference_depth
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ReasoningEvaluator;

impl ReasoningEvaluator {
    pub fn evaluate(&self, state: &ReasoningState) -> ReasoningScore {
        let relation_count = state.semantic_graph.relations.len() as f64;
        let labels = state
            .semantic_graph
            .concepts
            .values()
            .map(|concept| concept.label.as_str())
            .collect::<Vec<_>>();
        let semantic_coherence = (relation_count / 8.0).clamp(0.0, 1.0);
        let design_hits = ["stateless", "layered_architecture", "api_gateway"]
            .into_iter()
            .filter(|label| labels.contains(label))
            .count() as f64;
        let design_alignment = (design_hits / 3.0).clamp(0.0, 1.0);
        let inference_depth = (state.inferred_relations.len() as f64 / 6.0).clamp(0.0, 1.0);
        ReasoningScore {
            semantic_coherence,
            design_alignment,
            inference_depth,
        }
    }
}
