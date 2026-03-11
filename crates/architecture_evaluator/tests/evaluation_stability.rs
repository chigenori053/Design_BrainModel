use architecture_evaluator::{ArchitectureEvaluator, DefaultArchitectureEvaluator};
use architecture_reasoner::ReverseArchitectureReasoner;
use architecture_state_v2::ArchitectureState;
use code_ir::CodeIr;
use design_domain::DesignUnit;

#[test]
fn test13_evaluation_stability() {
    let graph = {
        let mut gateway = DesignUnit::new(1, "ApiGateway");
        gateway.dependencies.push(design_domain::DesignUnitId(2));
        let mut service = DesignUnit::new(2, "UserService");
        service.dependencies.push(design_domain::DesignUnitId(3));
        let repository = DesignUnit::new(3, "UserRepository");
        ReverseArchitectureReasoner
            .infer_from_code_ir(&CodeIr::from_design_units(&[gateway, service, repository]))
    };
    let state = ArchitectureState {
        problem: "stable api ranking".into(),
        architecture_graph: graph,
        ..ArchitectureState::default()
    };
    let evaluator = DefaultArchitectureEvaluator;
    let first = evaluator.evaluate_score(&state);
    let second = evaluator.evaluate_score(&state);

    assert_eq!(first, second);
}
