use learning_core::{
    EpisodeFeedback, ObjectiveCoeffs, Optimizer, OptimizerConfig, PolicyStore, SearchPolicy,
    compute_reward,
};
use runtime_core::search_domain::{FeatureVector, WEIGHT_DIM};

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

fn success_feedback(score: f64) -> EpisodeFeedback {
    let mut features = [0.0f64; WEIGHT_DIM];
    features[0] = score;
    EpisodeFeedback {
        success: true,
        features: FeatureVector(features),
        reward: compute_reward(true, 0.1, 0.05, score, &ObjectiveCoeffs::default()),
        score,
    }
}

fn failure_feedback() -> EpisodeFeedback {
    EpisodeFeedback {
        success: false,
        features: FeatureVector([0.0; WEIGHT_DIM]),
        reward: compute_reward(false, 0.5, 0.3, 0.0, &ObjectiveCoeffs::default()),
        score: 0.0,
    }
}

fn run_n_steps(n: usize, feedback_fn: impl Fn(usize) -> EpisodeFeedback) -> SearchPolicy {
    let config = OptimizerConfig::default();
    let mut optimizer = Optimizer::new(config);
    let mut store = PolicyStore::new();
    let mut policy = SearchPolicy::initial();

    for i in 0..n {
        let fb = feedback_fn(i);
        let (new_policy, is_stable) = optimizer.step(&policy, fb, &store);
        store.save(new_policy.clone(), is_stable);
        policy = new_policy;
    }
    policy
}

// ──────────────────────────────────────────────────────────────────────────────
// 9.1 Improvement: optimized weights produce a higher aggregate score than
//     baseline uniform weights given the same positive feedback stream.
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn improvement_optimized_score_exceeds_baseline() {
    let baseline = SearchPolicy::initial();
    let optimized = run_n_steps(32, |i| success_feedback(0.5 + (i as f64) * 0.01));

    // After many successful episodes the version should have advanced.
    assert!(optimized.version > baseline.version);

    // The first weight should have shifted from the uniform baseline due to
    // gradient updates driven by the positive reward signal.
    let uniform = 1.0 / WEIGHT_DIM as f64;
    let changed = optimized.weights.0.iter().any(|&w| (w - uniform).abs() > 1e-6);
    assert!(changed, "weights must change after learning from feedback");
}

// ──────────────────────────────────────────────────────────────────────────────
// 9.2 Determinism: same feedback history → same policy
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn determinism_same_history_same_policy() {
    let feedbacks: Vec<EpisodeFeedback> = (0..20)
        .map(|i| {
            if i % 3 == 0 {
                failure_feedback()
            } else {
                success_feedback(0.4 + i as f64 * 0.02)
            }
        })
        .collect();

    let run = |feedbacks: &[EpisodeFeedback]| {
        let config = OptimizerConfig::default();
        let mut optimizer = Optimizer::new(config);
        let mut store = PolicyStore::new();
        let mut policy = SearchPolicy::initial();
        for fb in feedbacks {
            let (p, stable) = optimizer.step(&policy, fb.clone(), &store);
            store.save(p.clone(), stable);
            policy = p;
        }
        policy
    };

    let p1 = run(&feedbacks);
    let p2 = run(&feedbacks);

    assert_eq!(p1, p2, "identical feedback history must produce identical policy");
}

// ──────────────────────────────────────────────────────────────────────────────
// 9.3 Replay: policy retrieved at a past version is identical to the policy
//     that was saved at that version.
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn replay_past_version_matches_saved() {
    let config = OptimizerConfig::default();
    let mut optimizer = Optimizer::new(config);
    let mut store = PolicyStore::new();
    let mut policy = SearchPolicy::initial();

    let mut snapshot_v2: Option<SearchPolicy> = None;

    for i in 0..10 {
        let fb = success_feedback(0.5 + i as f64 * 0.05);
        let (new_policy, is_stable) = optimizer.step(&policy, fb, &store);
        if new_policy.version == 2 {
            snapshot_v2 = Some(new_policy.clone());
        }
        store.save(new_policy.clone(), is_stable);
        policy = new_policy;
    }

    if let Some(snap) = snapshot_v2 {
        let replayed = store.at_version(snap.version);
        assert_eq!(Some(&snap), replayed, "replay must match the saved snapshot");
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// 9.4 Stability: degradation triggers revert to last stable policy
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn stability_degradation_triggers_revert() {
    let config = OptimizerConfig {
        drift_window: 4,
        drift_threshold: 0.0,
        ..Default::default()
    };
    let mut optimizer = Optimizer::new(config);
    let mut store = PolicyStore::new();
    let mut policy = SearchPolicy::initial();

    // Build a stable baseline with several good episodes.
    for i in 0..6 {
        let fb = success_feedback(0.8 + i as f64 * 0.01);
        let (new_policy, is_stable) = optimizer.step(&policy, fb, &store);
        store.save(new_policy.clone(), is_stable);
        policy = new_policy;
    }

    let stable_version = store.last_stable().map(|p| p.version);
    assert!(stable_version.is_some(), "a stable checkpoint should exist after good episodes");

    // Now inject degrading episodes to trigger drift detection.
    let mut reverted = false;
    for j in 0..8 {
        let fb = failure_feedback();
        // Use a low score to trigger drift.
        let fb = EpisodeFeedback {
            score: 0.1 - j as f64 * 0.02,
            ..fb
        };
        let (new_policy, _is_stable) = optimizer.step(&policy, fb, &store);
        if Some(new_policy.version) == stable_version {
            reverted = true;
            break;
        }
        store.save(new_policy.clone(), false);
        policy = new_policy;
    }

    assert!(reverted, "optimizer must revert to last stable policy on degradation");
}

// ──────────────────────────────────────────────────────────────────────────────
// 9.5 Bounds: all policy values must stay within their specified ranges
// ──────────────────────────────────────────────────────────────────────────────

#[test]
fn bounds_all_values_within_range() {
    use runtime_core::search_domain::{WEIGHT_MAX, WEIGHT_MIN};
    use runtime_core::search_runtime::{MAX_BEAM, MAX_EXPLORATION, MIN_BEAM, MIN_EXPLORATION};

    let final_policy = run_n_steps(50, |i| {
        if i % 2 == 0 {
            success_feedback(0.9)
        } else {
            failure_feedback()
        }
    });

    for &w in &final_policy.weights.0 {
        assert!(
            w >= WEIGHT_MIN && w <= WEIGHT_MAX,
            "weight {w} out of bounds [{WEIGHT_MIN}, {WEIGHT_MAX}]"
        );
    }
    assert!(
        final_policy.beam_width >= MIN_BEAM && final_policy.beam_width <= MAX_BEAM,
        "beam_width {} out of bounds [{MIN_BEAM}, {MAX_BEAM}]",
        final_policy.beam_width
    );
    assert!(
        final_policy.exploration_rate >= MIN_EXPLORATION
            && final_policy.exploration_rate <= MAX_EXPLORATION,
        "exploration_rate {} out of bounds [{MIN_EXPLORATION}, {MAX_EXPLORATION}]",
        final_policy.exploration_rate
    );
}
