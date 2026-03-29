use agent_core::{
    BetaProfile, HvPolicy, IntentProfile, Phase1Config, WorldModelMode, run_phase1_matrix,
};

#[test]
fn hv_policy_legacy_and_guided_produce_same_row_schema() {
    let legacy = run(HvPolicy::Legacy);
    let guided = run(HvPolicy::Guided);

    assert!(!legacy.is_empty());
    assert!(!guided.is_empty());

    let l = &legacy[0];
    let g = &guided[0];
    assert!(!l.variant.is_empty());
    assert!(!g.variant.is_empty());
    assert_eq!(l.objective_vector_raw.split('|').count(), 4);
    assert_eq!(g.objective_vector_raw.split('|').count(), 4);
}

fn run(policy: HvPolicy) -> Vec<agent_core::Phase1RawRow> {
    let cfg = Phase1Config {
        beam_width: 5,
        max_steps: 8,
        hv_policy: policy,
        seed: 42,
        world_model_enabled: true,
        world_model_alpha: 0.7,
        world_model_beta: 0.3,
        world_model_beta_profile: BetaProfile::Balanced,
        world_model_actions_per_state: 5,
        world_model_max_depth: 1,
        intent_profile: IntentProfile::Balanced,
        world_model_mode: WorldModelMode::Deterministic,
        world_model_variance_penalty: 0.2,
        world_model_semantic_variance_penalty: 0.15,
        world_model_semantic_variance_max_penalty: 0.35,
        world_model_learning_rate: 0.1,
        world_model_learning_decay: 0.05,
        world_model_learning_confidence_gate: 0.55,
        world_model_confidence_floor: 0.2,
        norm_alpha: 0.1,
        alpha: 3.0,
        temperature: 0.1,
        entropy_beta: 0.03,
        lambda_min: 0.2,
        lambda_target_entropy: 1.2,
        lambda_k: 0.2,
        lambda_ema: 0.4,
    };
    let (rows, _) = run_phase1_matrix(cfg);
    rows
}
