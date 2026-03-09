use architecture_domain::ArchitectureState;
use evaluation_engine::EvaluationResult;
use language_core::SemanticGraph;
use memory_graph::DesignExperienceGraph;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExperienceState {
    pub graph: DesignExperienceGraph,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationState {
    pub latest: Option<EvaluationResult>,
    pub history: Vec<EvaluationResult>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RuntimeState {
    pub request_id: String,
    pub stage: String,
    pub event_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AIContext {
    pub architecture_state: ArchitectureState,
    pub semantic_graph: SemanticGraph,
    pub experience_state: ExperienceState,
    pub evaluation_state: EvaluationState,
    pub runtime_state: RuntimeState,
}

impl AIContext {
    pub fn new(
        architecture_state: ArchitectureState,
        semantic_graph: SemanticGraph,
        experience_state: ExperienceState,
        evaluation_state: EvaluationState,
        runtime_state: RuntimeState,
    ) -> Self {
        Self {
            architecture_state,
            semantic_graph,
            experience_state,
            evaluation_state,
            runtime_state,
        }
    }
}
