use semantic_dhm::{ConceptId, ConceptQuery, ConceptUnit, ResonanceWeights, resonance};

#[derive(Clone, Debug, PartialEq)]
pub struct ResonanceReport {
    pub c1: ConceptId,
    pub c2: ConceptId,
    pub score: f32,
    pub v_sim: f32,
    pub s_sim: f32,
    pub a_diff: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Explanation {
    pub summary: String,
    pub reasoning: String,
    pub abstraction_note: String,
}

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

#[derive(Clone, Debug, PartialEq)]
pub struct RecommendationInput {
    pub query: ConceptUnit,
    pub candidates: Vec<ConceptUnit>,
    pub top_k: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionType {
    Merge,
    Refine,
    ApplyPattern,
    Separate,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Recommendation {
    pub target: ConceptId,
    pub action: ActionType,
    pub score: f32,
    pub rationale: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecommendationReport {
    pub summary: String,
    pub recommendations: Vec<Recommendation>,
}

#[derive(Default)]
pub struct Recomposer;

impl Recomposer {
    pub fn explain_concept(&self, c: &ConceptUnit) -> Explanation {
        let abs = abstraction_phrase(c.a);
        Explanation {
            summary: format!("This concept represents a {abs}."),
            reasoning: format!("Abstraction score = {:.2}.", round2(c.a)),
            abstraction_note: format!("Abstraction level: {abs}."),
        }
    }

    pub fn explain_resonance(&self, report: &ResonanceReport) -> Explanation {
        let align = alignment_phrase(report.score);
        let mut abstraction_note = String::new();
        if report.s_sim >= 0.6 {
            abstraction_note.push_str("with significant structural similarity");
        }
        if report.a_diff >= 0.4 {
            if !abstraction_note.is_empty() {
                abstraction_note.push_str("; ");
            }
            abstraction_note.push_str("at different abstraction levels");
        }
        if abstraction_note.is_empty() {
            abstraction_note.push_str("no additional structural or abstraction note");
        }
        Explanation {
            summary: format!("Concept A is {align} with Concept B."),
            reasoning: format!(
                "Semantic similarity = {:.2}, structural similarity = {:.2}, abstraction difference = {:.2}.",
                round2(report.v_sim),
                round2(report.s_sim),
                round2(report.a_diff),
            ),
            abstraction_note,
        }
    }

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

    pub fn recommend(
        &self,
        input: &RecommendationInput,
        weights: &ResonanceWeights,
    ) -> RecommendationReport {
        let q = ConceptQuery {
            v: input.query.v.clone(),
            a: input.query.a,
            s: input.query.s.clone(),
        }
        .normalized();

        let mut candidates = input.candidates.clone();
        candidates.sort_by(|l, r| l.id.cmp(&r.id));

        let mut recommendations = candidates
            .into_iter()
            .map(|c| recommend_one(&q, &c, weights))
            .collect::<Vec<_>>();

        recommendations.sort_by(|l, r| {
            r.score
                .partial_cmp(&l.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| l.target.cmp(&r.target))
        });
        recommendations.truncate(input.top_k);

        let summary = recommendation_summary(&recommendations);
        RecommendationReport {
            summary,
            recommendations,
        }
    }
}

fn recommend_one(
    query: &ConceptQuery,
    c: &ConceptUnit,
    weights: &ResonanceWeights,
) -> Recommendation {
    let v_sim = dot_norm(&query.v, &c.v);
    let s_sim = dot_norm(&query.s, &c.s);
    let a_diff = (query.a - c.a).abs();
    let r = resonance(query, c, *weights);

    let action = if r >= 0.60 && a_diff < 0.40 {
        ActionType::Merge
    } else if r >= 0.60 && a_diff >= 0.40 {
        ActionType::Refine
    } else if (0.10..0.60).contains(&r) && s_sim >= 0.60 {
        ActionType::ApplyPattern
    } else if r < -0.10 {
        ActionType::Separate
    } else if r >= 0.10 {
        ActionType::ApplyPattern
    } else {
        ActionType::Separate
    };

    let rationale = match action {
        ActionType::Merge => format!(
            "Consider merging with Concept {}. Strong alignment detected (R={:.2}).",
            c.id.0,
            round2(r)
        ),
        ActionType::Refine => format!(
            "Consider refining abstraction alignment with Concept {}. High resonance but abstraction gap detected (Δa={:.2}).",
            c.id.0,
            round2(a_diff)
        ),
        ActionType::ApplyPattern => format!(
            "Consider applying structural pattern from Concept {}. Structural similarity detected (s_sim={:.2}).",
            c.id.0,
            round2(s_sim)
        ),
        ActionType::Separate => format!(
            "Consider separating from Concept {}. Structural conflict detected (R={:.2}).",
            c.id.0,
            round2(r)
        ),
    };

    let _ = v_sim;

    Recommendation {
        target: c.id,
        action,
        score: round2(r),
        rationale,
    }
}

fn recommendation_summary(recs: &[Recommendation]) -> String {
    let mut merge_refine = 0usize;
    let mut apply = 0usize;
    let mut separate = 0usize;

    for r in recs {
        match r.action {
            ActionType::Merge | ActionType::Refine => merge_refine += 1,
            ActionType::ApplyPattern => apply += 1,
            ActionType::Separate => separate += 1,
        }
    }

    if merge_refine > apply && merge_refine > separate {
        "Strong integration opportunities detected.".to_string()
    } else if apply > merge_refine && apply > separate {
        "Structural reuse opportunities detected.".to_string()
    } else if separate > merge_refine && separate > apply {
        "Conflict areas require attention.".to_string()
    } else {
        "Mixed structural signals detected.".to_string()
    }
}

fn abstraction_phrase(a: f32) -> &'static str {
    if a < 0.30 {
        "concrete design element"
    } else if a < 0.70 {
        "mid-level structural concept"
    } else {
        "high-level architectural abstraction"
    }
}

fn alignment_phrase(score: f32) -> &'static str {
    if score >= 0.75 {
        "strongly aligned"
    } else if score >= 0.40 {
        "moderately aligned"
    } else if score >= 0.10 {
        "weakly aligned"
    } else if score > -0.10 {
        "structurally neutral"
    } else {
        "structurally conflicting"
    }
}

fn coherence_phrase(r_mean: f32) -> &'static str {
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

fn abstraction_tendency_phrase(a_mean: f32) -> &'static str {
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

fn dot_norm(a: &[f32], b: &[f32]) -> f32 {
    let an = normalize(a);
    let bn = normalize(b);
    an.iter().zip(bn.iter()).map(|(x, y)| x * y).sum::<f32>()
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

fn round2(v: f32) -> f32 {
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use semantic_dhm::{ConceptQuery, SemanticDhm};

    use super::{
        ActionType, MultiConceptInput, RecommendationInput, Recomposer, ResonanceReport,
        abstraction_phrase, abstraction_tendency_phrase, alignment_phrase, coherence_phrase,
    };

    fn sample_query(a: f32, seed: f32) -> ConceptQuery {
        let mut v = vec![0.0f32; 384];
        let mut s = vec![0.0f32; 384];
        v[0] = seed;
        v[1] = 1.0 - seed;
        s[0] = 1.0 - seed;
        s[1] = seed;
        ConceptQuery { v, a, s }
    }

    #[test]
    fn abstraction_boundary_test() {
        assert_eq!(abstraction_phrase(0.29), "concrete design element");
        assert_eq!(abstraction_phrase(0.30), "mid-level structural concept");
        assert_eq!(
            abstraction_phrase(0.70),
            "high-level architectural abstraction"
        );
    }

    #[test]
    fn resonance_classification_test() {
        assert_eq!(alignment_phrase(0.80), "strongly aligned");
        assert_eq!(alignment_phrase(0.50), "moderately aligned");
        assert_eq!(alignment_phrase(0.20), "weakly aligned");
        assert_eq!(alignment_phrase(0.00), "structurally neutral");
        assert_eq!(alignment_phrase(-0.20), "structurally conflicting");
    }

    #[test]
    fn deterministic_output_test() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id = dhm.insert_query(&sample_query(0.45, 0.8));
        let c = dhm.get(id).expect("concept");
        let r = Recomposer;
        let e1 = r.explain_concept(&c);
        let e2 = r.explain_concept(&c);
        assert_eq!(e1, e2);

        let rep = ResonanceReport {
            c1: c.id,
            c2: c.id,
            score: 0.42,
            v_sim: 0.51,
            s_sim: 0.61,
            a_diff: 0.10,
        };
        let x1 = r.explain_resonance(&rep);
        let x2 = r.explain_resonance(&rep);
        assert_eq!(x1, x2);
    }

    #[test]
    fn non_mutation_test() {
        let path = std::env::temp_dir().join(format!(
            "recomposer_non_mutation_{}.bin",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut dhm = SemanticDhm::file(&path).expect("open");
        let id = dhm.insert_query(&sample_query(0.33, 0.7));
        let before = dhm.get(id).expect("before");

        let rc = Recomposer;
        let _ = rc.explain_concept(&before);

        let q = sample_query(0.33, 0.7);
        let score = dhm.recall(&q, 1).first().map(|(_, s)| *s).unwrap_or(0.0);

        let rep = ResonanceReport {
            c1: id,
            c2: id,
            score,
            v_sim: 1.0,
            s_sim: 1.0,
            a_diff: 0.0,
        };
        let _ = rc.explain_resonance(&rep);

        let after = dhm.get(id).expect("after");
        assert_eq!(before, after);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn multi_order_invariance_test() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&sample_query(0.2, 0.9));
        let id2 = dhm.insert_query(&sample_query(0.8, 0.1));
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

    #[test]
    fn multi_conflict_detection_test() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let q1 = ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = 1.0;
                v
            },
            a: 0.1,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = 1.0;
                s
            },
        };
        let q2 = ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = -1.0;
                v
            },
            a: 0.9,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = -1.0;
                s
            },
        };
        let id1 = dhm.insert_query(&q1);
        let id2 = dhm.insert_query(&q2);
        let c1 = dhm.get(id1).expect("c1");
        let c2 = dhm.get(id2).expect("c2");

        let r = Recomposer;
        let out = r.explain_multiple(
            &MultiConceptInput {
                concepts: vec![c1, c2],
                weights: None,
            },
            &dhm.weights(),
        );
        assert!(out.metrics.conflict_pairs > 0);
    }

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
    fn recommend_merge_refine_apply_separate_and_priority() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let q_id = dhm.insert_query(&ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = 1.0;
                v
            },
            a: 0.20,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = 1.0;
                s
            },
        });
        let query = dhm.get(q_id).expect("q");

        let c_merge_id = dhm.insert_query(&ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = 0.98;
                v[1] = 0.02;
                v
            },
            a: 0.25,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = 0.97;
                s[1] = 0.03;
                s
            },
        });
        let c_refine_id = dhm.insert_query(&ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = 0.98;
                v[1] = 0.02;
                v
            },
            a: 0.85,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = 0.96;
                s[1] = 0.04;
                s
            },
        });
        let c_apply_id = dhm.insert_query(&ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = 0.20;
                v[1] = 0.98;
                v
            },
            a: 0.25,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = 0.80;
                s[1] = 0.60;
                s
            },
        });
        let c_sep_id = dhm.insert_query(&ConceptQuery {
            v: {
                let mut v = vec![0.0; 384];
                v[0] = -0.9;
                v[1] = -0.1;
                v
            },
            a: 0.95,
            s: {
                let mut s = vec![0.0; 384];
                s[0] = -0.9;
                s[1] = -0.1;
                s
            },
        });

        let r = Recomposer;
        let rec = r.recommend(
            &RecommendationInput {
                query,
                candidates: vec![
                    dhm.get(c_refine_id).expect("r"),
                    dhm.get(c_sep_id).expect("s"),
                    dhm.get(c_merge_id).expect("m"),
                    dhm.get(c_apply_id).expect("a"),
                ],
                top_k: 10,
            },
            &dhm.weights(),
        );

        let action_of = |id| {
            rec.recommendations
                .iter()
                .find(|x| x.target == id)
                .map(|x| x.action)
        };

        assert_eq!(action_of(c_merge_id), Some(ActionType::Merge));
        assert_eq!(action_of(c_refine_id), Some(ActionType::Refine));
        assert_eq!(action_of(c_apply_id), Some(ActionType::ApplyPattern));
        assert_eq!(action_of(c_sep_id), Some(ActionType::Separate));
    }

    #[test]
    fn recommend_top_k_and_deterministic() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let q_id = dhm.insert_query(&sample_query(0.40, 0.70));
        let query = dhm.get(q_id).expect("q");

        let mut candidates = Vec::new();
        for i in 0..5 {
            let id = dhm.insert_query(&sample_query(
                0.40 + 0.01 * i as f32,
                0.70 - 0.05 * i as f32,
            ));
            candidates.push(dhm.get(id).expect("c"));
        }

        let r = Recomposer;
        let a = r.recommend(
            &RecommendationInput {
                query: query.clone(),
                candidates: candidates.clone(),
                top_k: 3,
            },
            &dhm.weights(),
        );
        let b = r.recommend(
            &RecommendationInput {
                query,
                candidates,
                top_k: 3,
            },
            &dhm.weights(),
        );
        assert_eq!(a, b);
        assert_eq!(a.recommendations.len(), 3);
    }
}
