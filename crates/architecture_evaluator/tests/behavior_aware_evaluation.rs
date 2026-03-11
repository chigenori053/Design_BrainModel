use architecture_evaluator::DefaultArchitectureEvaluator;
use architecture_reasoner::ReverseArchitectureReasoner;
use architecture_state_v2::ArchitectureState;
use code_ir::CodeIr;
use design_domain::DesignUnit;
use workload_model::{Distribution, WorkloadModel};

#[test]
fn behavior_aware_evaluation_produces_v3_score() {
    let mut gateway = DesignUnit::new(1, "ApiGateway");
    gateway.dependencies.push(design_domain::DesignUnitId(2));
    let service = DesignUnit::new(2, "WorkerService");
    let graph = ReverseArchitectureReasoner
        .infer_from_code_ir(&CodeIr::from_design_units(&[gateway, service]));
    let state = ArchitectureState {
        problem: "behavior aware api".into(),
        architecture_graph: graph,
        ..ArchitectureState::default()
    };
    let workload = WorkloadModel {
        request_rate: 100.0,
        concurrency: 16,
        request_distribution: Distribution::Uniform,
    };

    let details = DefaultArchitectureEvaluator.evaluate_v3(&state, &workload, None);

    assert!(details.behavior.is_some());
    assert!(
        details
            .score_v3
            .as_ref()
            .map(|score| score.total())
            .unwrap_or_default()
            > 0.0
    );
}
