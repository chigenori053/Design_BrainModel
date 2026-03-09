use concept_engine::ConceptId;
use design_search_engine::{
    BeamSearchStrategy, ConstraintEngine, DesignSearchEngine, DesignState, DesignStateId,
    Evaluator, IntentNode, SearchConfig,
};
use memory_space_complex::ComplexField;

#[test]
fn constraint_filtering() {
    let engine = DesignSearchEngine {
        strategy: Box::new(BeamSearchStrategy),
        evaluator: Evaluator,
        constraint_engine: ConstraintEngine {
            intent_nodes: vec![IntentNode {
                concept: ConceptId::from_name("LIMITED"),
                weight: 1,
            }],
        },
        config: SearchConfig {
            beam_width: 8,
            max_depth: 20,
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
    assert!(out.states.values().all(|s| s.design_units.len() <= 8));
}
