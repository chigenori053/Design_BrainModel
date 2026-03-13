use architecture_domain::ArchitectureState;
use design_grammar::GrammarValidation;
use evaluation_engine::EvaluationResult;
use math_reasoning_engine::MathReasoningTrace;
use world_model_core::Action;
use world_model_core::WorldState;

/// Phase9-D search state: wraps a WorldState with search metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct SearchState {
    pub state_id: u64,
    pub world_state: WorldState,
    pub architecture_state: ArchitectureState,
    pub evaluation_result: Option<EvaluationResult>,
    pub depth: usize,
    pub score: f64,
    pub prior_score: f64,
    pub policy_score: f64,
    pub pareto_rank: usize,
    pub source_action: Option<Action>,
    pub grammar_validation: Option<GrammarValidation>,
    pub math_reasoning: Option<MathReasoningTrace>,
}

impl SearchState {
    pub fn new(state_id: u64, world_state: WorldState) -> Self {
        Self {
            state_id,
            world_state,
            architecture_state: ArchitectureState::default(),
            evaluation_result: None,
            depth: 0,
            score: 0.0,
            prior_score: 1.0,
            policy_score: 0.0,
            pareto_rank: 0,
            source_action: None,
            grammar_validation: None,
            math_reasoning: None,
        }
    }
}
