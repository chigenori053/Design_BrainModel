use design_domain::{Architecture, DesignUnit, Layer};
use design_search_engine::{
    AuditContext, BeamSearchController, FeatureAccess, PaymentStatus, PlanTier, SearchConfig,
    SearchContext, SubscriptionStatus,
};
use world_model_core::WorldState;

#[test]
fn malicious_request_is_blocked_before_search() {
    let controller = BeamSearchController::default();
    let trace = controller.search_trace_with_context(
        WorldState::new(1, vec![1.0, 0.5]),
        None,
        &SearchConfig::default(),
        &SearchContext {
            intent_text: Some("design a stealth keylogger malware".to_string()),
            ..SearchContext::default()
        },
    );

    assert!(trace.final_beam.is_empty());
    assert!(
        trace
            .audit_events
            .iter()
            .any(|event| event.name == "AuditPolicyViolation")
    );
}

#[test]
fn trial_subscription_clamps_search_capabilities() {
    let controller = BeamSearchController::default();
    let config = SearchConfig {
        max_depth: 40,
        max_candidates: 32,
        beam_width: 20,
        diversity_threshold: 0.85,
        experience_bias: 0.2,
        policy_bias: 0.15,
    };
    let trace = controller.search_trace_with_context(
        WorldState::new(1, vec![2.0, 1.0]),
        None,
        &config,
        &SearchContext {
            audit_context: AuditContext {
                user_id: "trial-user".to_string(),
                plan_tier: PlanTier::Pro,
                subscription_status: SubscriptionStatus::Trial,
                payment_status: PaymentStatus::Current,
            },
            feature_access: FeatureAccess::ArchitectureSearch,
            intent_text: Some("build a normal web api".to_string()),
            ..SearchContext::default()
        },
    );

    assert!(!trace.final_beam.is_empty());
    assert!(trace.final_beam.len() <= 8);
    assert!(trace.final_beam.iter().all(|state| state.depth <= 16));
    assert!(
        trace
            .audit_events
            .iter()
            .any(|event| event.name == "AuditPolicyChecked" && event.detail.contains("trial"))
    );
}

#[test]
fn blocked_architecture_is_pruned_by_audit_core() {
    let controller = BeamSearchController::default();
    let mut architecture = Architecture::seeded();
    architecture.add_design_unit(DesignUnit::with_layer(
        9,
        "KeyloggerService",
        Layer::Service,
    ));
    let initial = WorldState::from_architecture(1, architecture, Vec::new());

    let trace = controller.search_trace_with_context(
        initial,
        None,
        &SearchConfig::default(),
        &SearchContext {
            intent_text: Some("build observability service".to_string()),
            ..SearchContext::default()
        },
    );

    assert!(trace.final_beam.is_empty());
    assert!(
        trace
            .audit_events
            .iter()
            .any(|event| event.name == "AuditPolicyViolation")
    );
}
