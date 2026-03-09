use semantic_domain::{Intent, MeaningGraph};

use crate::{semantic_field::SemanticField, semantic_graph::SemanticGraph};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LanguageState {
    pub semantic_graph: SemanticGraph,
    pub semantic_field: SemanticField,
    pub intent: Option<Intent>,
    pub generated_sentence: Option<String>,
    pub source_text: String,
}

impl LanguageState {
    pub fn to_meaning_graph(&self) -> MeaningGraph {
        Self::semantic_graph_to_meaning_graph(&self.semantic_graph, self.intent.clone())
    }

    pub fn semantic_graph_to_meaning_graph(
        semantic_graph: &SemanticGraph,
        intent: Option<Intent>,
    ) -> MeaningGraph {
        let semantic_units = semantic_graph
            .concepts
            .values()
            .cloned()
            .map(|concept| semantic_domain::SemanticUnit {
                id: concept.concept_id.0,
                concept: semantic_domain::Concept {
                    name: concept.label,
                    weight: 1.0,
                },
                mapped_design_unit: None,
            })
            .collect();
        let relations = semantic_graph
            .relations
            .iter()
            .map(|relation| semantic_domain::SemanticRelation {
                from: relation.source.0,
                to: relation.target.0,
                label: format!("{:?}", relation.relation),
            })
            .collect();
        MeaningGraph {
            intents: intent.into_iter().collect(),
            semantic_units,
            relations,
        }
    }
}
