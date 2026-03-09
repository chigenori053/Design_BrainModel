use crate::LanguageState;

#[derive(Clone, Debug, PartialEq)]
pub struct LanguageScore {
    pub semantic_consistency: f64,
    pub intent_alignment: f64,
    pub design_alignment: f64,
}

impl LanguageScore {
    pub fn total(&self) -> f64 {
        ((self.semantic_consistency + self.intent_alignment + self.design_alignment) / 3.0)
            .clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct LanguageEvaluator;

impl LanguageEvaluator {
    pub fn evaluate(&self, state: &LanguageState) -> LanguageScore {
        let concept_count = state.semantic_graph.concepts.len() as f64;
        let relation_count = state.semantic_graph.relations.len() as f64;
        let semantic_consistency = if concept_count <= 0.0 {
            0.0
        } else {
            (relation_count / concept_count.max(1.0)).clamp(0.0, 1.0)
        };
        let intent_alignment = if state.intent.is_some() { 1.0 } else { 0.2 };
        let labels = state
            .semantic_graph
            .concepts
            .values()
            .map(|concept| concept.label.as_str())
            .collect::<Vec<_>>();
        let design_alignment = if labels.contains(&"api")
            && (labels.contains(&"service") || labels.contains(&"database"))
        {
            1.0
        } else if labels.contains(&"api") {
            0.7
        } else {
            0.3
        };
        LanguageScore {
            semantic_consistency,
            intent_alignment,
            design_alignment,
        }
    }
}
