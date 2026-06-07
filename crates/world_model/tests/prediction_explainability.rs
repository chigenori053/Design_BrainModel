use world_model::{
    CausalStability, CauseCategory, CauseSeverity, MutationValidationGate, PredictionExplainer,
    PredictionResult, ValidationDecision,
};

fn prediction(
    stability: CausalStability,
    consequence_score: f64,
    affected_entities: &[&str],
) -> PredictionResult {
    PredictionResult {
        stability,
        consequence_score,
        affected_entities: affected_entities
            .iter()
            .map(|entity| (*entity).to_string())
            .collect(),
        warnings: Vec::new(),
    }
}

#[test]
fn stable_prediction_has_no_critical_causes() {
    let explanation = PredictionExplainer::explain(&prediction(
        CausalStability {
            causal_entropy: 0.1,
            future_divergence: 0.1,
            world_consistency: 1.0,
            semantic_drift: 0.1,
        },
        0.9,
        &[],
    ));

    assert!(
        explanation
            .primary_causes
            .iter()
            .all(|cause| cause.severity != CauseSeverity::Critical)
    );
}

#[test]
fn degrading_prediction_generates_cause_and_recommendation() {
    let explanation = PredictionExplainer::explain(&prediction(
        CausalStability {
            causal_entropy: 0.5,
            future_divergence: 0.4,
            world_consistency: 0.8,
            semantic_drift: 0.2,
        },
        0.6,
        &["runtime"],
    ));

    assert!(!explanation.primary_causes.is_empty());
    assert!(!explanation.recommendations.is_empty());
}

#[test]
fn unstable_prediction_has_multiple_causes_and_ranked_entities() {
    let explanation = PredictionExplainer::explain(&prediction(
        CausalStability {
            causal_entropy: 0.7,
            future_divergence: 0.8,
            world_consistency: 0.6,
            semantic_drift: 0.7,
        },
        0.2,
        &["coding", "core", "runtime"],
    ));

    assert!(explanation.primary_causes.len() >= 2);
    assert!(
        explanation
            .affected_entities
            .windows(2)
            .all(|pair| pair[0].impact_score >= pair[1].impact_score)
    );
}

#[test]
fn contradictory_prediction_contains_causal_conflict() {
    let explanation = PredictionExplainer::explain(&prediction(
        CausalStability {
            causal_entropy: 1.0,
            future_divergence: 1.0,
            world_consistency: 0.0,
            semantic_drift: 1.0,
        },
        0.0,
        &["runtime", "policy"],
    ));

    assert!(
        explanation
            .primary_causes
            .iter()
            .any(|cause| cause.category == CauseCategory::CausalConflict)
    );
}

#[test]
fn explanation_is_deterministic() {
    let prediction = prediction(
        CausalStability {
            causal_entropy: 0.7,
            future_divergence: 0.8,
            world_consistency: 0.6,
            semantic_drift: 0.7,
        },
        0.2,
        &["runtime", "core"],
    );

    assert_eq!(
        PredictionExplainer::explain(&prediction),
        PredictionExplainer::explain(&prediction)
    );
}

#[test]
fn narrative_contains_causes_impacts_and_recommendations() {
    let prediction = prediction(
        CausalStability {
            causal_entropy: 1.0,
            future_divergence: 1.0,
            world_consistency: 0.0,
            semantic_drift: 1.0,
        },
        0.0,
        &["runtime", "policy"],
    );
    let decision = MutationValidationGate::decide(&prediction);
    let explanation = PredictionExplainer::explain(&prediction);
    let narrative = MutationValidationGate::narrative(decision, &explanation);

    assert_eq!(decision, ValidationDecision::Reject);
    assert!(narrative.contains("主な原因"));
    assert!(narrative.contains("影響が予測される領域"));
    assert!(narrative.contains("推奨事項"));
}
