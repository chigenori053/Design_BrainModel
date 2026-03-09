use ai_context::AIContext;
use memory_space_core::RecallResult;
use world_model_core::{ConsistencyScore, Hypothesis, WorldState};

use crate::event::RuntimeEventBus;
use crate::modality::ModalityInput;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RequestId(pub String);

/// Phase9-D extended pipeline stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RuntimeStage {
    #[default]
    Input,
    Normalize,
    Recall,
    HypothesisGeneration,
    /// Phase9-D: architecture search via BeamSearchController
    Search,
    /// Phase11: architecture simulation via WorldModel
    Simulation,
    /// Phase9-D: architecture evaluation
    Evaluation,
    /// Phase9-D: candidate ranking
    Ranking,
    TransitionEvaluation,
    ConsistencyEvaluation,
    Output,
}

/// Search results stored in context after Phase9-D search stage.
#[derive(Debug, Clone, Default)]
pub struct SearchSummary {
    pub search_states: usize,
    pub best_score: f64,
    pub best_simulation_score: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SearchMetrics {
    pub explored_states: usize,
    pub unique_architectures: usize,
    pub pattern_matches: usize,
    pub policy_score_mean: f64,
    pub architecture_similarity: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Phase9RuntimeContext {
    pub request_id: RequestId,
    pub modality_input: ModalityInput,
    pub recall_result: Option<RecallResult>,
    pub world_state: Option<WorldState>,
    pub hypotheses: Vec<Hypothesis>,
    pub evaluation: Option<ConsistencyScore>,
    pub stage: RuntimeStage,
    pub event_bus: RuntimeEventBus,
    /// Phase9-D: populated after the Search → Ranking stages.
    pub search_summary: Option<SearchSummary>,
    pub search_metrics: Option<SearchMetrics>,
    pub ai_context: Option<AIContext>,
}

impl Phase9RuntimeContext {
    pub fn advance(&mut self, stage: RuntimeStage) {
        self.stage = stage;
    }
}
