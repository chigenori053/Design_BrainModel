use agent_core::{HvPolicy, Phase1Config, run_phase1_matrix};

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
