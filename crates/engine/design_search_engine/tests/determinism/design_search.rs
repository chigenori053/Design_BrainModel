use concept_engine::ConceptId;
use design_search_engine::{
    BeamSearchStrategy, ConstraintEngine, DesignSearchEngine, DesignState, DesignStateId,
    Evaluator, SearchConfig,
};
use memory_space_complex::ComplexField;

#[test]
fn design_search_determinism() {
    let engine = DesignSearchEngine {
        strategy: Box::new(BeamSearchStrategy),
        evaluator: Evaluator,
        constraint_engine: ConstraintEngine::default(),
        config: SearchConfig::default(),
    };
    let initial = DesignState {
        id: DesignStateId(1),
        design_units: Vec::new(),
        evaluation: None,
        state_vector: ComplexField::new(Vec::new()),
    };
    let concepts = vec![ConceptId::from_name("DATABASE")];

    let a = engine.search(initial.clone(), &concepts);
    let b = engine.search(initial, &concepts);

    assert_eq!(a.states.len(), b.states.len());
    assert_eq!(a.edges, b.edges);
}
