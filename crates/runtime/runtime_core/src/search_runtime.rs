use crate::search_core::{BeamSearchState, candidate_to_scored, effective_beam_width, run_search};
use crate::search_domain::{SearchInput, SearchPolicy, SearchResult, apply_score_weights};

/// Unified search entry point.
///
/// Policy is a required argument — implicit policy is prohibited.
/// All policy application (beam width, exploration, scoring) happens
/// here in runtime_core; design_search_engine remains policy-free.
pub fn search(input: SearchInput, policy: &SearchPolicy) -> SearchResult {
    let beam_width = effective_beam_width(policy);
    let state = BeamSearchState::default();

    let raw = run_search(&input, beam_width, &state);

    if raw.beam.is_empty() {
        return SearchResult::default();
    }

    // Apply policy scoring: score = base_score + dot(features, weights).
    // Features: [prior_score, policy_score].
    let mut states: Vec<_> = raw
        .beam
        .into_iter()
        .map(|c| {
            let mut scored = candidate_to_scored(c);
            let features = [scored.prior_score, scored.policy_score];
            scored.score = apply_score_weights(scored.base_score, &features, &policy.weights);
            scored
        })
        .collect();

    // Exploration: if rate > 0, inject lower-ranked candidates deterministically.
    // Preserves full order — no randomness.
    if policy.exploration_rate > 0.0 {
        inject_lower_ranked_candidates(&mut states, &input, policy, beam_width);
    }

    // Re-sort by final score descending, then state_id for stability.
    states.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.world_state.state_id.cmp(&b.world_state.state_id))
    });

    SearchResult {
        states,
        explored_count: raw.explored_count,
        depth_best_scores: raw.depth_best_scores,
    }
}

/// Deterministic exploration: inject candidates that were pruned from the
/// beam but still score above the floor derived from exploration_rate.
/// No random selection — all decisions are based on score order.
fn inject_lower_ranked_candidates(
    beam: &mut Vec<crate::search_domain::ScoredState>,
    input: &SearchInput,
    policy: &SearchPolicy,
    beam_width: usize,
) {
    // Run a wider search to surface previously-pruned states.
    use crate::search_core::{BeamSearchState, run_search};
    use crate::search_domain::MAX_BEAM;

    let wide_width = ((beam_width as f64 * (1.0 + policy.exploration_rate)).round() as usize)
        .min(MAX_BEAM);

    if wide_width <= beam_width {
        return;
    }

    let state = BeamSearchState::default();
    let wide_raw = run_search(input, wide_width, &state);

    // Collect IDs already in the beam.
    let existing_ids: std::collections::HashSet<u64> =
        beam.iter().map(|s| s.world_state.state_id).collect();

    // Score cutoff: the lowest score currently in the beam.
    let floor = beam
        .iter()
        .map(|s| s.score)
        .fold(f64::INFINITY, f64::min);

    // Append states from the wider beam that are not already present and
    // that score at or above the floor (maintain quality bound).
    for c in wide_raw.beam {
        if existing_ids.contains(&c.world_state.state_id) {
            continue;
        }
        let features = [c.prior_score, c.policy_score];
        let scored_val =
            apply_score_weights(c.base_score, &features, &policy.weights);
        if scored_val >= floor {
            let mut s = candidate_to_scored(c);
            s.score = scored_val;
            beam.push(s);
        }
    }
}
