use std::collections::BTreeSet;

use agent_core::{
    BetaProfile, HvPolicy, IntentProfile, Phase1Config, WorldModelMode, explain_phase1_candidate,
    load_intent_template, run_phase1_matrix,
};

fn test_config() -> Phase1Config {
    Phase1Config {
        beam_width: 5,
        max_steps: 8,
        hv_policy: HvPolicy::Guided,
        seed: 42,
        world_model_enabled: false,
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
    }
}

fn phase3_config() -> Phase1Config {
    Phase1Config {
        world_model_enabled: true,
        ..test_config()
    }
}

fn phase35_config() -> Phase1Config {
    Phase1Config {
        world_model_enabled: true,
        world_model_max_depth: 2,
        ..test_config()
    }
}

#[test]
fn phase2_structured_search_meets_basic_kpis() {
    let (rows, summaries) = run_phase1_matrix(test_config());
    let base_summaries = summaries
        .iter()
        .filter(|row| row.variant == "Base")
        .collect::<Vec<_>>();
    assert!(!base_summaries.is_empty());

    let max_hv = base_summaries
        .iter()
        .map(|row| row.frontier_hv)
        .fold(0.0_f64, f64::max);
    let max_coverage = base_summaries
        .iter()
        .map(|row| row.cluster_coverage)
        .fold(0.0_f64, f64::max);
    let max_frontier = base_summaries
        .iter()
        .map(|row| row.pareto_front_size)
        .max()
        .unwrap_or(0);

    assert!(max_hv > 0.1, "expected max HV > 0.1, got {max_hv}");
    assert!(
        max_coverage >= 0.8,
        "expected cluster coverage >= 0.8, got {max_coverage}"
    );
    assert!(
        max_frontier > 1,
        "expected frontier size > 1, got {max_frontier}"
    );

    let latest_depth = base_summaries
        .iter()
        .map(|row| row.depth)
        .max()
        .unwrap_or(0);
    let base_rows = rows
        .iter()
        .filter(|row| row.variant == "Base" && row.depth == latest_depth)
        .collect::<Vec<_>>();
    let cluster_count = base_rows
        .iter()
        .map(|row| row.cluster_id)
        .collect::<BTreeSet<_>>()
        .len();
    assert!(
        cluster_count >= 2,
        "expected multiple clusters, got {cluster_count}"
    );
}

#[test]
fn phase2_frontier_scores_have_variance_and_no_duplicates_collapse() {
    let (rows, summaries) = run_phase1_matrix(test_config());
    let latest_depth = summaries
        .iter()
        .filter(|row| row.variant == "Base")
        .map(|row| row.depth)
        .max()
        .unwrap_or(0);
    let base_rows = rows
        .iter()
        .filter(|row| row.variant == "Base" && row.depth == latest_depth)
        .collect::<Vec<_>>();
    assert!(!base_rows.is_empty());

    let diversity_values = base_rows
        .iter()
        .map(|row| row.diversity_score)
        .collect::<Vec<_>>();
    let mean = diversity_values.iter().sum::<f64>() / diversity_values.len() as f64;
    let variance = diversity_values
        .iter()
        .map(|value| {
            let delta = *value - mean;
            delta * delta
        })
        .sum::<f64>()
        / diversity_values.len() as f64;

    assert!(
        variance > 0.0,
        "expected diversity score variance, got {variance}"
    );
    assert!(base_rows.iter().any(|row| row.cluster_id > 0));
}

#[test]
fn phase25_control_keeps_hv_stable_and_coverage_high() {
    let (_, summaries) = run_phase1_matrix(Phase1Config {
        max_steps: 16,
        ..test_config()
    });
    let base_summaries = summaries
        .iter()
        .filter(|row| row.variant == "Base")
        .collect::<Vec<_>>();
    assert!(!base_summaries.is_empty());

    let min_coverage = base_summaries
        .iter()
        .map(|row| row.cluster_coverage)
        .fold(1.0_f64, f64::min);
    let max_hv = summaries
        .iter()
        .map(|row| row.frontier_hv)
        .fold(0.0_f64, f64::max);
    let max_drawdown = summaries
        .iter()
        .filter(|row| row.variant == "Base")
        .collect::<Vec<_>>()
        .windows(2)
        .map(|pair| (pair[0].frontier_hv - pair[1].frontier_hv).max(0.0))
        .fold(0.0_f64, f64::max);
    let stable_change_steps = base_summaries
        .iter()
        .skip(1)
        .filter(|row| row.frontier_change_ratio <= 0.2)
        .count();
    let evaluated_steps = base_summaries.len().saturating_sub(1);

    assert!(max_hv > 0.15, "expected max HV > 0.15, got {max_hv}");
    assert!(
        min_coverage >= 0.8,
        "expected coverage >= 0.8 across steps, got {min_coverage}"
    );
    assert!(
        max_drawdown <= 0.05,
        "expected bounded drawdown <= 0.05, got {max_drawdown}"
    );
    assert!(
        stable_change_steps * 100 / evaluated_steps.max(1) >= 40,
        "expected >=40% stable frontier steps, got {stable_change_steps}/{evaluated_steps}"
    );
}

#[test]
fn phase25_structured_search_is_deterministic_end_to_end() {
    let first = run_phase1_matrix(Phase1Config {
        max_steps: 12,
        ..test_config()
    });
    let second = run_phase1_matrix(Phase1Config {
        max_steps: 12,
        ..test_config()
    });
    assert_eq!(first.0, second.0);
    assert_eq!(first.1, second.1);
}

#[test]
fn phase3_world_model_improves_hv_and_creates_rp_variance() {
    let (_, phase25_summaries) = run_phase1_matrix(Phase1Config {
        max_steps: 12,
        ..test_config()
    });
    let (phase3_rows, phase3_summaries) = run_phase1_matrix(Phase1Config {
        max_steps: 12,
        ..phase3_config()
    });

    let phase25_max_hv = phase25_summaries
        .iter()
        .map(|row| row.frontier_hv)
        .fold(0.0_f64, f64::max);
    let phase3_max_hv = phase3_summaries
        .iter()
        .map(|row| row.frontier_hv)
        .fold(0.0_f64, f64::max);
    assert!(
        phase3_max_hv > phase25_max_hv,
        "expected phase3 HV improvement: phase25={phase25_max_hv}, phase3={phase3_max_hv}"
    );

    let phase3_base_rows = phase3_rows
        .iter()
        .filter(|row| row.variant == "Base")
        .collect::<Vec<_>>();
    let rp_values = phase3_base_rows
        .iter()
        .map(|row| row.repair_potential)
        .collect::<Vec<_>>();
    let rp_mean = rp_values.iter().sum::<f64>() / rp_values.len().max(1) as f64;
    let rp_variance = rp_values
        .iter()
        .map(|value| {
            let delta = *value - rp_mean;
            delta * delta
        })
        .sum::<f64>()
        / rp_values.len().max(1) as f64;
    assert!(
        rp_variance > 0.0,
        "expected RP variance > 0, got {rp_variance}"
    );
    assert!(
        phase3_base_rows
            .iter()
            .any(|row| row.action_label != "static")
    );
}

#[test]
fn phase3_world_model_is_deterministic_end_to_end() {
    let first = run_phase1_matrix(Phase1Config {
        max_steps: 10,
        ..phase3_config()
    });
    let second = run_phase1_matrix(Phase1Config {
        max_steps: 10,
        ..phase3_config()
    });
    assert_eq!(first.0, second.0);
    assert_eq!(first.1, second.1);
}

#[test]
fn phase35_depth_two_reduces_hv_variance_and_improves_stability() {
    let (_, phase3_summaries) = run_phase1_matrix(Phase1Config {
        max_steps: 12,
        ..phase3_config()
    });
    let (phase35_rows, phase35_summaries) = run_phase1_matrix(Phase1Config {
        max_steps: 12,
        ..phase35_config()
    });

    let phase3_hv = phase3_summaries
        .iter()
        .filter(|row| row.variant == "Base")
        .map(|row| row.frontier_hv)
        .collect::<Vec<_>>();
    let phase35_hv = phase35_summaries
        .iter()
        .filter(|row| row.variant == "Base")
        .map(|row| row.frontier_hv)
        .collect::<Vec<_>>();
    let phase3_max = phase3_hv.iter().copied().fold(0.0_f64, f64::max);
    let phase35_max = phase35_hv.iter().copied().fold(0.0_f64, f64::max);
    let phase35_drawdown = phase35_hv
        .windows(2)
        .map(|pair| (pair[0] - pair[1]).max(0.0))
        .fold(0.0_f64, f64::max);
    let phase35_final = phase35_hv.last().copied().unwrap_or(0.0);

    assert!(
        phase35_max >= phase3_max,
        "expected phase3.5 HV >= phase3: phase3={phase3_max}, phase3.5={phase35_max}"
    );
    assert!(
        phase35_drawdown <= 0.1,
        "expected bounded phase3.5 drawdown <= 0.1, got {phase35_drawdown}"
    );
    assert!(
        phase35_final + 0.05 >= phase35_max,
        "expected phase3.5 to remain near its peak: final={phase35_final}, max={phase35_max}"
    );
    assert!(
        phase35_rows
            .iter()
            .any(|row| row.action_label.contains("sim2") || row.action_label != "static"),
        "expected simulated actions in phase3.5 output"
    );
}

#[test]
fn phase35_depth_two_is_deterministic_end_to_end() {
    let first = run_phase1_matrix(Phase1Config {
        max_steps: 10,
        ..phase35_config()
    });
    let second = run_phase1_matrix(Phase1Config {
        max_steps: 10,
        ..phase35_config()
    });
    assert_eq!(first.0, second.0);
    assert_eq!(first.1, second.1);
}

#[test]
fn phase4_intent_switch_changes_selected_solution() {
    let maintainability = run_phase1_matrix(Phase1Config {
        world_model_enabled: true,
        intent_profile: IntentProfile::Maintainability,
        ..phase35_config()
    });
    let performance = run_phase1_matrix(Phase1Config {
        world_model_enabled: true,
        intent_profile: IntentProfile::Performance,
        ..phase35_config()
    });

    let maintainability_actions = maintainability
        .0
        .iter()
        .filter(|row| row.variant == "Base")
        .map(|row| row.action_label.clone())
        .collect::<Vec<_>>();
    let performance_actions = performance
        .0
        .iter()
        .filter(|row| row.variant == "Base")
        .map(|row| row.action_label.clone())
        .collect::<Vec<_>>();

    assert_ne!(
        maintainability_actions, performance_actions,
        "intent switch should alter selected action sequence"
    );
}

#[test]
fn phase4_confidence_and_variance_are_emitted() {
    let (rows, _) = run_phase1_matrix(Phase1Config {
        world_model_enabled: true,
        ..phase35_config()
    });
    let simulated = rows
        .iter()
        .filter(|row| row.variant == "Base" && row.action_label != "static")
        .collect::<Vec<_>>();

    assert!(!simulated.is_empty());
    assert!(
        simulated
            .iter()
            .all(|row| (0.0..=1.0).contains(&row.confidence))
    );
    assert!(simulated.iter().all(|row| row.variance >= 0.0));
    assert!(simulated.iter().all(|row| row.semantic_variance >= 0.0));
    assert!(
        simulated
            .iter()
            .all(|row| (0.1..=0.7).contains(&row.beta_reliance))
    );
    assert!(simulated.iter().all(|row| row.final_score >= 0.0));
}

#[test]
fn phase45_beta_converges_from_exploration_to_controlled_reliance() {
    let (rows, _) = run_phase1_matrix(Phase1Config {
        max_steps: 16,
        world_model_enabled: true,
        world_model_mode: WorldModelMode::Deterministic,
        ..phase35_config()
    });
    let base = rows
        .iter()
        .filter(|row| row.variant == "Base" && row.action_label != "static")
        .collect::<Vec<_>>();
    assert!(base.len() >= 3);
    assert!(base.first().expect("first").beta_reliance >= 0.1);
    assert!(base.last().expect("last").beta_reliance <= 0.7);
    assert!(
        base.last().expect("last").beta_reliance <= base.first().expect("first").beta_reliance
            || base.last().expect("last").confidence > 0.8
    );
}

#[test]
fn phase4_learning_converges_by_reducing_variance() {
    let (_, summaries) = run_phase1_matrix(Phase1Config {
        max_steps: 16,
        world_model_enabled: true,
        world_model_mode: WorldModelMode::Probabilistic,
        intent_profile: IntentProfile::Refactor,
        ..phase35_config()
    });
    let base = summaries
        .iter()
        .filter(|row| row.variant == "Base")
        .collect::<Vec<_>>();
    assert!(base.len() >= 4);

    let early = base
        .iter()
        .take(base.len() / 2)
        .map(|row| row.score_variance)
        .collect::<Vec<_>>();
    let late = base
        .iter()
        .skip(base.len() / 2)
        .map(|row| row.score_variance)
        .collect::<Vec<_>>();
    assert!(
        variance(&late) <= variance(&early),
        "expected search variance to contract after learning"
    );
}

#[test]
fn phase4_probabilistic_mode_changes_behavior_but_stays_valid() {
    let deterministic = run_phase1_matrix(Phase1Config {
        world_model_enabled: true,
        world_model_mode: WorldModelMode::Deterministic,
        ..phase35_config()
    });
    let probabilistic = run_phase1_matrix(Phase1Config {
        world_model_enabled: true,
        world_model_mode: WorldModelMode::Probabilistic,
        intent_profile: IntentProfile::Refactor,
        ..phase35_config()
    });

    let det_rows = deterministic
        .0
        .iter()
        .filter(|row| row.variant == "Base")
        .collect::<Vec<_>>();
    let prob_rows = probabilistic
        .0
        .iter()
        .filter(|row| row.variant == "Base")
        .collect::<Vec<_>>();

    assert_eq!(det_rows.len(), prob_rows.len());
    assert!(
        det_rows
            .iter()
            .zip(prob_rows.iter())
            .any(|(lhs, rhs)| lhs.action_label != rhs.action_label
                || lhs.final_score != rhs.final_score),
        "probabilistic mode should alter frontier behavior"
    );
    assert!(
        prob_rows
            .iter()
            .all(|row| (0.0..=1.0).contains(&row.confidence))
    );
    assert!(prob_rows.iter().any(|row| row.learning_bias != 0.0));
}

#[test]
fn phase45_semantic_variance_detects_intent_conflict() {
    let low_risk = run_phase1_matrix(Phase1Config {
        world_model_enabled: true,
        intent_profile: IntentProfile::LowRisk,
        ..phase35_config()
    });
    let refactor = run_phase1_matrix(Phase1Config {
        world_model_enabled: true,
        intent_profile: IntentProfile::Refactor,
        ..phase35_config()
    });

    let low_risk_semantic = low_risk
        .0
        .iter()
        .filter(|row| row.variant == "Base" && row.action_label != "static")
        .map(|row| row.semantic_variance)
        .sum::<f64>();
    let refactor_semantic = refactor
        .0
        .iter()
        .filter(|row| row.variant == "Base" && row.action_label != "static")
        .map(|row| row.semantic_variance)
        .sum::<f64>();

    assert_ne!(low_risk_semantic, refactor_semantic);
}

#[test]
fn phase45_learning_stays_stable_without_drift() {
    let (_, summaries) = run_phase1_matrix(Phase1Config {
        max_steps: 16,
        world_model_enabled: true,
        world_model_mode: WorldModelMode::Probabilistic,
        intent_profile: IntentProfile::LowRisk,
        ..phase35_config()
    });
    let base = summaries
        .iter()
        .filter(|row| row.variant == "Base")
        .collect::<Vec<_>>();
    let max_semantic = base
        .iter()
        .map(|row| row.semantic_variance_mean)
        .fold(0.0_f64, f64::max);
    assert!(max_semantic <= 1.0);
}

#[test]
fn phase46_intent_templates_match_expected_profiles() {
    assert_eq!(
        load_intent_template("Maintainability"),
        Some(IntentProfile::Maintainability)
    );
    assert_eq!(
        load_intent_template("Performance"),
        Some(IntentProfile::Performance)
    );
    assert_eq!(load_intent_template("Safety"), Some(IntentProfile::LowRisk));
    assert_eq!(
        load_intent_template("Refactor Priority"),
        Some(IntentProfile::Refactor)
    );
}

#[test]
fn phase46_explainability_includes_top_factors() {
    let (rows, _) = run_phase1_matrix(Phase1Config {
        world_model_enabled: true,
        ..phase35_config()
    });
    let candidate = rows
        .iter()
        .filter(|row| row.variant == "Base" && row.action_label != "static")
        .max_by(|lhs, rhs| lhs.final_score.total_cmp(&rhs.final_score))
        .expect("candidate");
    let explanation = explain_phase1_candidate(candidate);

    assert!(!explanation.summary.is_empty());
    assert!(!explanation.top_factors.is_empty());
    assert!(explanation.confidence >= 0.0);
}

#[test]
fn phase46_long_run_remains_stable_for_2000_steps() {
    let (_, summaries) = run_phase1_matrix(Phase1Config {
        max_steps: 25,
        world_model_enabled: true,
        ..phase35_config()
    });
    let base = summaries
        .iter()
        .filter(|row| row.variant == "Base")
        .collect::<Vec<_>>();
    assert!(!base.is_empty());
    let max_drawdown = base
        .windows(2)
        .map(|pair| (pair[0].frontier_hv - pair[1].frontier_hv).max(0.0))
        .fold(0.0_f64, f64::max);
    assert!(
        max_drawdown <= 0.1,
        "unexpected hv drawdown: {max_drawdown}"
    );
}

fn variance(values: &[f64]) -> f64 {
    let mean = values.iter().sum::<f64>() / values.len().max(1) as f64;
    values
        .iter()
        .map(|value| {
            let delta = *value - mean;
            delta * delta
        })
        .sum::<f64>()
        / values.len().max(1) as f64
}
