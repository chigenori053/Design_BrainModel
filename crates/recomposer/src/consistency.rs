use semantic_dhm::{ConceptId, ConceptQuery, ConceptUnit, ResonanceWeights, resonance};

use crate::constants::{
    DIRECTIONAL_CONFLICT_NEG_THRESHOLD, DIRECTIONAL_CONFLICT_POS_THRESHOLD,
    STRUCTURAL_CONFLICT_THRESHOLD,
};
use crate::recommend::dot_norm;

#[derive(Clone, Debug, PartialEq)]
pub struct TradeoffDetail {
    pub pair: (ConceptId, ConceptId),
    pub tension: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConsistencyReport {
    pub directional_conflicts: usize,
    pub structural_conflicts: usize,
    pub tradeoffs: Vec<TradeoffDetail>,
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
                tradeoffs: Vec::new(),
                stability_score: 1.0,
            },
            global_coherence: 1.0,
        };
    }

    let mut sorted = concepts.to_vec();
    sorted.sort_by(|l, r| l.id.cmp(&r.id));

    let mut directional_conflicts = 0usize;
    let mut structural_conflicts = 0usize;
    let mut tradeoffs = Vec::new();
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
                polarity: c1.polarity,
            }
            .normalized();
            let r = resonance(&query, c2, *weights);
            let v_sim = dot_norm(&c1.v, &c2.v);
            let s_sim = dot_norm(&c1.s, &c2.s);
            let a_diff = (c1.a - c2.a).abs();

            let directional_conflict = (v_sim >= DIRECTIONAL_CONFLICT_POS_THRESHOLD
                && s_sim <= DIRECTIONAL_CONFLICT_NEG_THRESHOLD)
                || (v_sim <= DIRECTIONAL_CONFLICT_NEG_THRESHOLD
                    && s_sim >= DIRECTIONAL_CONFLICT_POS_THRESHOLD);
            if directional_conflict {
                directional_conflicts += 1;
            }

            if (c1.polarity as i16) * (c2.polarity as i16) < 0 {
                structural_conflicts += 1;
                r_sum += r;
                pair_count += 1;
                continue;
            }

            if r < STRUCTURAL_CONFLICT_THRESHOLD {
                structural_conflicts += 1;
                r_sum += r;
                pair_count += 1;
                continue;
            }

            let t_a = a_diff;
            let t_s = (-r).max(0.0);
            let tension = (t_a * t_a + t_s * t_s).sqrt();
            if tension > 0.30 {
                tradeoffs.push(TradeoffDetail {
                    pair: (c1.id, c2.id),
                    tension,
                });
            }

            r_sum += r;
            pair_count += 1;
        }
    }

    let pair_f = pair_count as f32;
    let global_coherence = if pair_count == 0 { 1.0 } else { r_sum / pair_f };
    let penalty = (structural_conflicts as f32)
        + (directional_conflicts as f32)
        + (tradeoffs.len() as f32 * 0.5);
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

#[cfg(test)]
mod tests {
    use semantic_dhm::{ConceptQuery, SemanticDhm};

    use super::compute_consistency;
    use crate::constants::STRUCTURAL_CONFLICT_THRESHOLD;

    fn query(v0: f32, v1: f32, a: f32, s0: f32, s1: f32) -> ConceptQuery {
        let mut v = vec![0.0f32; 384];
        let mut s = vec![0.0f32; 384];
        v[0] = v0;
        v[1] = v1;
        s[0] = s0;
        s[1] = s1;
        ConceptQuery {
            v,
            a,
            s,
            polarity: 0,
        }
    }

    #[test]
    fn a_abstract_gap_detects_tradeoff() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&query(1.0, 0.0, 0.10, 1.0, 0.0));
        let id2 = dhm.insert_query(&query(1.0, 0.0, 0.60, 1.0, 0.0));
        let concepts = vec![dhm.get(id1).expect("c1"), dhm.get(id2).expect("c2")];

        let out = compute_consistency(&concepts, &dhm.weights());
        assert_eq!(out.report.structural_conflicts, 0);
        assert_eq!(out.report.tradeoffs.len(), 1);
    }

    #[test]
    fn b_polarity_opposition_is_structural_conflict() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&query(1.0, 0.0, 0.20, 1.0, 0.0));
        let id2 = dhm.insert_query(&query(0.0, 1.0, 0.20, 1.0, 0.0));
        let mut c1 = dhm.get(id1).expect("c1");
        let mut c2 = dhm.get(id2).expect("c2");
        c1.polarity = -1;
        c2.polarity = 1;

        let out = compute_consistency(&[c1, c2], &dhm.weights());
        assert_eq!(out.report.structural_conflicts, 1);
        assert!(out.report.tradeoffs.is_empty());
    }

    #[test]
    fn c_structural_conflict_not_tradeoff() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&query(1.0, 0.0, 0.20, 1.0, 0.0));
        let id2 = dhm.insert_query(&query(-1.0, 0.0, 0.20, -1.0, 0.0));
        let concepts = vec![dhm.get(id1).expect("c1"), dhm.get(id2).expect("c2")];

        let out = compute_consistency(&concepts, &dhm.weights());
        assert_eq!(out.report.structural_conflicts, 1);
        assert!(out.report.tradeoffs.is_empty());
    }

    #[test]
    fn f_polarity_reversal_detected_as_structural_conflict() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&query(1.0, 0.0, 0.30, 1.0, 0.0));
        // opposite polarity must be conflict regardless of embedding gate.
        let id2 = dhm.insert_query(&query(1.0, 0.0, 0.90, -1.0, 0.0));

        let mut c1 = dhm.get(id1).expect("c1");
        let mut c2 = dhm.get(id2).expect("c2");
        c1.polarity = 1;
        c2.polarity = -1;

        let out = compute_consistency(&[c1, c2], &dhm.weights());
        assert_eq!(out.report.structural_conflicts, 1);
        assert!(out.report.tradeoffs.is_empty());
    }

    #[test]
    fn d_opposite_direction_still_allows_tradeoff_in_v2_1() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&query(1.0, 0.0, 0.10, 1.0, 0.0));
        let id2 = dhm.insert_query(&query(1.0, 0.0, 0.60, -1.0, 0.0));
        let concepts = vec![dhm.get(id1).expect("c1"), dhm.get(id2).expect("c2")];

        let out = compute_consistency(&concepts, &dhm.weights());
        assert_eq!(out.report.directional_conflicts, 1);
        assert_eq!(out.report.tradeoffs.len(), 1);
    }

    #[test]
    fn e_tension_boundary() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&query(1.0, 0.0, 0.10, 1.0, 0.0));

        // T = 0.30 => not detected (strictly >)
        let id2 = dhm.insert_query(&query(1.0, 0.0, 0.40, 1.0, 0.0));
        let concepts = vec![dhm.get(id1).expect("c1"), dhm.get(id2).expect("c2")];
        let out = compute_consistency(&concepts, &dhm.weights());
        assert!(out.report.tradeoffs.is_empty());

        // T = 0.30001 => detected
        let id3 = dhm.insert_query(&query(1.0, 0.0, 0.40001, 1.0, 0.0));
        let concepts2 = vec![dhm.get(id1).expect("c1"), dhm.get(id3).expect("c3")];
        let out2 = compute_consistency(&concepts2, &dhm.weights());
        assert_eq!(out2.report.tradeoffs.len(), 1);
    }

    #[test]
    fn threshold_constant_matches_spec() {
        let boundary = "-0.15".parse::<f32>().expect("boundary");
        let below = "-0.15001".parse::<f32>().expect("below");
        assert!(boundary >= STRUCTURAL_CONFLICT_THRESHOLD);
        assert!(below < STRUCTURAL_CONFLICT_THRESHOLD);
    }
}
