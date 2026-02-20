use semantic_dhm::{ConceptQuery, ConceptUnit, ResonanceWeights, resonance};

use crate::recommend::dot_norm;

#[derive(Clone, Debug, PartialEq)]
pub struct ConsistencyReport {
    pub directional_conflicts: usize,
    pub structural_conflicts: usize,
    pub tradeoffs: usize,
    pub stability_score: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct ConsistencyComputation {
    pub report: ConsistencyReport,
    pub global_coherence: f32,
}

pub(crate) fn compute_consistency(
    concepts: &[ConceptUnit],
    weights: &ResonanceWeights,
) -> ConsistencyComputation {
    if concepts.len() < 2 {
        return ConsistencyComputation {
            report: ConsistencyReport {
                directional_conflicts: 0,
                structural_conflicts: 0,
                tradeoffs: 0,
                stability_score: 1.0,
            },
            global_coherence: 1.0,
        };
    }

    let mut sorted = concepts.to_vec();
    sorted.sort_by(|l, r| l.id.cmp(&r.id));

    let mut directional_conflicts = 0usize;
    let mut structural_conflicts = 0usize;
    let mut tradeoffs = 0usize;
    let mut r_sum = 0.0f32;
    let mut pair_count = 0usize;

    for i in 0..sorted.len() {
        for j in (i + 1)..sorted.len() {
            let c1 = &sorted[i];
            let c2 = &sorted[j];
            let query = ConceptQuery {
                v: c1.v.clone(),
                a: c1.a,
                s: c1.s.clone(),
            }
            .normalized();
            let r = resonance(&query, c2, *weights);
            let v_sim = dot_norm(&c1.v, &c2.v);
            let s_sim = dot_norm(&c1.s, &c2.s);
            let a_diff = (c1.a - c2.a).abs();

            if r < -0.10 {
                structural_conflicts += 1;
            }
            if (v_sim >= 0.10 && s_sim <= -0.10) || (v_sim <= -0.10 && s_sim >= 0.10) {
                directional_conflicts += 1;
            }
            if a_diff >= 0.40 && r >= 0.10 {
                tradeoffs += 1;
            }

            r_sum += r;
            pair_count += 1;
        }
    }

    let pair_f = pair_count as f32;
    let global_coherence = if pair_count == 0 { 1.0 } else { r_sum / pair_f };
    let penalty =
        (structural_conflicts as f32) + (directional_conflicts as f32) + (tradeoffs as f32 * 0.5);
    let stability_score = if pair_count == 0 {
        1.0
    } else {
        (1.0 - penalty / pair_f).clamp(0.0, 1.0)
    };

    ConsistencyComputation {
        report: ConsistencyReport {
            directional_conflicts,
            structural_conflicts,
            tradeoffs,
            stability_score,
        },
        global_coherence,
    }
}
