use concept_engine::ConceptId;
use design_search_engine::{
    BeamSearchStrategy, ConstraintEngine, DesignSearchEngine, DesignState, DesignStateId,
    Evaluator, SearchConfig,
};
use memory_space_complex::ComplexField;

#[test]
fn beam_search_selection() {
    let engine = DesignSearchEngine {
        strategy: Box::new(BeamSearchStrategy),
        evaluator: Evaluator,
        constraint_engine: ConstraintEngine::default(),
        config: SearchConfig {
            beam_width: 2,
            max_depth: 4,
            max_candidates: 64,
        },
    };

    let initial = DesignState {
        id: DesignStateId(1),
        design_units: Vec::new(),
        evaluation: None,
        state_vector: ComplexField::new(Vec::new()),
    };

    let out = engine.search(initial, &[ConceptId::from_name("DATABASE")]);
    assert!(!out.states.is_empty());
    assert!(!out.edges.is_empty());
}
