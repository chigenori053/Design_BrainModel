use semantic_dhm::{ConceptUnit, ResonanceWeights};

use crate::Recomposer;
use crate::explain::round2;
use crate::recommend::dot_norm;

#[derive(Clone, Debug, PartialEq)]
pub struct MultiConceptInput {
    pub concepts: Vec<ConceptUnit>,
    pub weights: Option<Vec<f32>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiExplanation {
    pub summary: String,
    pub structural_analysis: String,
    pub abstraction_analysis: String,
    pub conflict_analysis: String,
    pub metrics: MultiMetrics,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MultiMetrics {
    pub center_v_norm: String,
    pub mean_abstraction: String,
    pub pairwise_mean_r: String,
    pub conflict_pairs: usize,
}

impl Recomposer {
    pub fn explain_multiple(
        &self,
        input: &MultiConceptInput,
        resonance_weights: &ResonanceWeights,
    ) -> MultiExplanation {
        assert!(
            input.concepts.len() >= 2,
            "MultiConceptInput requires at least 2 concepts"
        );

        let normalized_weights = normalize_weights(input.weights.as_deref(), input.concepts.len());
        let mut pairs = input
            .concepts
            .iter()
            .cloned()
            .zip(normalized_weights)
            .collect::<Vec<_>>();

        pairs.sort_by(|(lc, _), (rc, _)| lc.id.cmp(&rc.id));

        let center = weighted_center_v(&pairs);
        let center_norm = l2_norm(&center);
        let a_mean = pairs
            .iter()
            .map(|(c, w)| c.a.clamp(0.0, 1.0) * *w)
            .sum::<f32>();

        let w = resonance_weights.normalized();
        let mut sum_r = 0.0f32;
        let mut pair_count = 0usize;
        let mut conflict_pairs = 0usize;

        for i in 0..pairs.len() {
            for j in (i + 1)..pairs.len() {
                let c1 = &pairs[i].0;
                let c2 = &pairs[j].0;
                let v_sim = dot_norm(&c1.v, &c2.v);
                let s_sim = dot_norm(&c1.s, &c2.s);
                let a_diff = (c1.a - c2.a).abs();
                let r = w.gamma1 * v_sim + w.gamma2 * s_sim - w.gamma3 * a_diff;
                if r < -0.10 {
                    conflict_pairs += 1;
                }
                sum_r += r;
                pair_count += 1;
            }
        }

        let r_mean = if pair_count == 0 {
            0.0
        } else {
            sum_r / pair_count as f32
        };

        let coherence = coherence_phrase(r_mean);
        let abstraction = abstraction_tendency_phrase(a_mean);
        let conflict = conflict_phrase(conflict_pairs);

        MultiExplanation {
            summary: format!("The provided concepts are {coherence} and are {abstraction}."),
            structural_analysis: format!(
                "Mean pairwise resonance = {:.2}. Computed structural center established (||v_center|| ≈ {:.2}).",
                round2(r_mean),
                round2(center_norm),
            ),
            abstraction_analysis: format!("Mean abstraction score = {:.2}.", round2(a_mean)),
            conflict_analysis: format!("{conflict} (count = {conflict_pairs})."),
            metrics: MultiMetrics {
                center_v_norm: format!("{:.2}", round2(center_norm)),
                mean_abstraction: format!("{:.2}", round2(a_mean)),
                pairwise_mean_r: format!("{:.2}", round2(r_mean)),
                conflict_pairs,
            },
        }
    }
}

pub(crate) fn coherence_phrase(r_mean: f32) -> &'static str {
    if r_mean >= 0.60 {
        "globally coherent"
    } else if r_mean >= 0.30 {
        "moderately coherent"
    } else if r_mean >= 0.0 {
        "loosely connected"
    } else {
        "structurally conflicting"
    }
}

pub(crate) fn abstraction_tendency_phrase(a_mean: f32) -> &'static str {
    if a_mean < 0.30 {
        "primarily concrete"
    } else if a_mean < 0.70 {
        "mixed abstraction levels"
    } else {
        "primarily high-level"
    }
}

fn conflict_phrase(conflict_pairs: usize) -> &'static str {
    if conflict_pairs == 0 {
        "no structural conflicts detected"
    } else if conflict_pairs <= 2 {
        "minor conflicts detected"
    } else {
        "multiple structural conflicts detected"
    }
}

fn normalize_weights(weights: Option<&[f32]>, n: usize) -> Vec<f32> {
    match weights {
        Some(ws) if ws.len() == n => {
            let clipped = ws.iter().map(|w| w.max(0.0)).collect::<Vec<_>>();
            let sum = clipped.iter().sum::<f32>();
            if sum <= f32::EPSILON {
                vec![1.0 / n as f32; n]
            } else {
                clipped.into_iter().map(|w| w / sum).collect()
            }
        }
        _ => vec![1.0 / n as f32; n],
    }
}

fn weighted_center_v(pairs: &[(ConceptUnit, f32)]) -> Vec<f32> {
    let dim = pairs.iter().map(|(c, _)| c.v.len()).max().unwrap_or(0);
    let mut acc = vec![0.0f32; dim];
    for (c, w) in pairs {
        let v = normalize(&c.v);
        for i in 0..dim.min(v.len()) {
            acc[i] += v[i] * *w;
        }
    }
    normalize(&acc)
}

fn normalize(v: &[f32]) -> Vec<f32> {
    let norm = l2_norm(v);
    if norm <= f32::EPSILON {
        return vec![0.0; v.len()];
    }
    v.iter().map(|x| x / norm).collect()
}

fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

#[cfg(test)]
mod tests {
    use semantic_dhm::{ConceptQuery, SemanticDhm};

    use super::{MultiConceptInput, Recomposer, abstraction_tendency_phrase, coherence_phrase};

    #[test]
    fn multi_boundary_value_test() {
        assert_eq!(coherence_phrase(0.30), "moderately coherent");
        assert_eq!(coherence_phrase(0.60), "globally coherent");
        assert_eq!(
            abstraction_tendency_phrase(0.30),
            "mixed abstraction levels"
        );
        assert_eq!(abstraction_tendency_phrase(0.70), "primarily high-level");
    }

    #[test]
    fn multi_order_invariance_test() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = 1.0;
                v
            },
            a: 0.2,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = 0.9;
                s
            },
            polarity: 0,
        });
        let id2 = dhm.insert_query(&ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = 0.2;
                v[1] = 1.0;
                v
            },
            a: 0.8,
            s: {
                let mut s = vec![0.0; 384];
                s[1] = 1.0;
                s
            },
            polarity: 0,
        });
        let c1 = dhm.get(id1).expect("c1");
        let c2 = dhm.get(id2).expect("c2");

        let r = Recomposer;
        let w = dhm.weights();

        let a = r.explain_multiple(
            &MultiConceptInput {
                concepts: vec![c1.clone(), c2.clone()],
                weights: None,
            },
            &w,
        );
        let b = r.explain_multiple(
            &MultiConceptInput {
                concepts: vec![c2, c1],
                weights: None,
            },
            &w,
        );
        assert_eq!(a, b);
    }
}
