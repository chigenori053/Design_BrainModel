use architecture_domain::ArchitectureState;
use design_domain::{Architecture, DesignUnit, Layer};
use evaluation_engine::EvaluationEngine;

#[test]
fn evaluation_engine_is_deterministic() {
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(1, "Controller", Layer::Ui));
    architecture.add_design_unit(DesignUnit::with_layer(2, "Service", Layer::Service));
    let state = ArchitectureState::from_architecture(&architecture, Vec::new());
    let engine = EvaluationEngine::default();
    let baseline = engine.evaluate(&state);

    for _ in 0..20 {
        assert_eq!(engine.evaluate(&state), baseline);
    }
}
