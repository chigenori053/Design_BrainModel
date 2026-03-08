use concept_engine::ConceptId;
use concept_field::ConceptField;
use design_search_engine::{DesignState, HypothesisGraph};
use memory_space_api::ConceptRecallHit;
use search_controller::SearchState;
use semantic_dhm::SemanticUnit;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntentNode {
    pub concept: ConceptId,
    pub weight: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IntentGraph {
    pub edges: Vec<(ConceptId, ConceptId)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeHypothesis {
    pub concept_a: ConceptId,
    pub concept_b: ConceptId,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeContext {
    pub input_text: String,
    pub semantic_units: Vec<SemanticUnit>,
    pub concepts: Vec<ConceptId>,
    pub intent_nodes: Vec<IntentNode>,
    pub concept_activation: Vec<(ConceptId, f32)>,
    pub concept_field: Option<ConceptField>,
    pub intent_graph: Option<IntentGraph>,
    pub memory_candidates: Vec<ConceptRecallHit>,
    pub search_state: Option<SearchState>,
    pub design_state: Option<DesignState>,
    pub hypothesis_graph: Option<HypothesisGraph>,
    pub hypotheses: Vec<RuntimeHypothesis>,
    pub tick: u64,
}
