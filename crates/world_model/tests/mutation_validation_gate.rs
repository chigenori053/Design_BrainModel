use design_domain::{Architecture, Dependency, DependencyKind, DesignUnit, DesignUnitId};
use world_model::{
    CausalStability, CausalStabilityLevel, MutationValidationGate, PredictionResult,
    ValidationDecision,
};
use world_model_core::WorldState;

fn prediction(stability: CausalStability) -> PredictionResult {
    PredictionResult {
        stability,
        consequence_score: 0.5,
        affected_entities: Vec::new(),
        warnings: Vec::new(),
    }
}

#[test]
fn stable_mutation_is_allowed() {
    let result = prediction(CausalStability {
        causal_entropy: 0.1,
        future_divergence: 0.1,
        world_consistency: 1.0,
        semantic_drift: 0.1,
    });

    assert_eq!(
        MutationValidationGate::classify(&result.stability),
        CausalStabilityLevel::Stable
    );
    assert_eq!(
        MutationValidationGate::decide(&result),
        ValidationDecision::Allow
    );
}

#[test]
fn degrading_mutation_produces_warning() {
    let result = prediction(CausalStability {
        causal_entropy: 0.5,
        future_divergence: 0.4,
        world_consistency: 0.8,
        semantic_drift: 0.2,
    });

    assert_eq!(
        MutationValidationGate::decide(&result),
        ValidationDecision::Warn
    );
}

#[test]
fn unstable_mutation_is_rejected() {
    let result = prediction(CausalStability {
        causal_entropy: 0.7,
        future_divergence: 0.8,
        world_consistency: 0.7,
        semantic_drift: 0.6,
    });

    assert_eq!(
        MutationValidationGate::decide(&result),
        ValidationDecision::Reject
    );
}

#[test]
fn contradictory_mutation_is_rejected() {
    let result = prediction(CausalStability {
        causal_entropy: 1.0,
        future_divergence: 1.0,
        world_consistency: 0.0,
        semantic_drift: 1.0,
    });

    assert_eq!(
        MutationValidationGate::classify(&result.stability),
        CausalStabilityLevel::Contradictory
    );
    assert_eq!(
        MutationValidationGate::decide(&result),
        ValidationDecision::Reject
    );
}

#[test]
fn prediction_is_deterministic() {
    let state = sample_world_state(&[(1, 2)]);

    assert_eq!(
        MutationValidationGate::predict(&state),
        MutationValidationGate::predict(&state)
    );
}

#[test]
fn adapter_and_prediction_pipeline_succeeds() {
    let result = MutationValidationGate::predict(&sample_world_state(&[(1, 2)]));

    assert!(result.consequence_score.is_finite());
    assert!(!result.affected_entities.is_empty());
}

fn sample_world_state(edges: &[(u64, u64)]) -> WorldState {
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::new(1, "Runtime"));
    architecture.add_design_unit(DesignUnit::new(2, "Policy"));
    for (from, to) in edges {
        architecture.dependencies.push(Dependency {
            from: DesignUnitId(*from),
            to: DesignUnitId(*to),
            kind: DependencyKind::Calls,
        });
        architecture.graph.edges.push((*from, *to));
    }
    WorldState::from_architecture(1, architecture, Vec::new())
}
