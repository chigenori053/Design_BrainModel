use concept_engine::ConceptId;
use concept_field::ConceptField;
use design_search_engine::{DesignState, HypothesisGraph, ReasoningResult};
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
    pub selected_template: Option<String>,
    pub memory_trace_ids: Vec<String>,
    pub evaluation_cache_hits: usize,
    pub reasoning_result: Option<ReasoningResult>,
    pub search_state: Option<SearchState>,
    pub design_state: Option<DesignState>,
    pub hypothesis_graph: Option<HypothesisGraph>,
    pub hypotheses: Vec<RuntimeHypothesis>,
    pub tick: u64,
}

impl RuntimeContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn release_completed_task_memory(&mut self) {
        self.memory_candidates.clear();
        self.memory_trace_ids.clear();
        self.reasoning_result = None;
        self.search_state = None;
        self.design_state = None;
        self.hypothesis_graph = None;
        self.hypotheses.clear();
        self.evaluation_cache_hits = 0;
    }

    pub fn force_clear_all(&mut self) {
        self.input_text.clear();
        self.semantic_units.clear();
        self.concepts.clear();
        self.intent_nodes.clear();
        self.concept_activation.clear();
        self.concept_field = None;
        self.intent_graph = None;
        self.release_completed_task_memory();
        self.selected_template = None;
        self.tick = 0;
    }
}
