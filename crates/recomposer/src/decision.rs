use semantic_dhm::{ConceptUnit, ResonanceWeights};

use crate::Recomposer;
use crate::consistency::compute_consistency;
use crate::explain::round2;

const STRUCTURAL_CONFLICT_WARNING: &str = "Decision unstable due to structural conflict";

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecisionWeights {
    pub coherence: f32,
    pub stability: f32,
    pub conflict: f32,
    pub tradeoff: f32,
}

impl DecisionWeights {
    pub fn clamped(self) -> Self {
        Self {
            coherence: self.coherence.max(0.0),
            stability: self.stability.max(0.0),
            conflict: self.conflict.max(0.0),
            tradeoff: self.tradeoff.max(0.0),
        }
    }

    pub fn total(self) -> f32 {
        self.coherence + self.stability + self.conflict + self.tradeoff
    }
}

impl Default for DecisionWeights {
    fn default() -> Self {
        Self {
            coherence: 0.4,
            stability: 0.3,
            conflict: 0.2,
            tradeoff: 0.1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecisionReport {
    pub decision_score: f32,
    pub weights: DecisionWeights,
    pub interpretation: String,
    pub warning: Option<String>,
}

impl Recomposer {
    pub fn decide(
        &self,
        concepts: &[ConceptUnit],
        weights: DecisionWeights,
        resonance_weights: &ResonanceWeights,
    ) -> DecisionReport {
        let safe_weights = weights.clamped();
        let consistency = compute_consistency(concepts, resonance_weights);
        let pair_count = pair_count(concepts.len()) as f32;

        let coherence_score = ((consistency.global_coherence + 1.0) / 2.0).clamp(0.0, 1.0);
        let stability_score = consistency.report.stability_score.clamp(0.0, 1.0);
        let conflict_score = if pair_count <= 0.0 {
            1.0
        } else {
            (1.0 - (consistency.report.structural_conflicts as f32 / pair_count)).clamp(0.0, 1.0)
        };
        let tradeoff_score = if pair_count <= 0.0 {
            1.0
        } else {
            (1.0 - (consistency.report.tradeoffs.len() as f32 / pair_count)).clamp(0.0, 1.0)
        };

        let weighted_sum = safe_weights.coherence * coherence_score
            + safe_weights.stability * stability_score
            + safe_weights.conflict * conflict_score
            + safe_weights.tradeoff * tradeoff_score;

        let decision_score = if safe_weights.total() <= f32::EPSILON {
            0.0
        } else {
            (weighted_sum / safe_weights.total()).clamp(0.0, 1.0)
        };

        let warning = if consistency.report.structural_conflicts > 0 {
            Some(STRUCTURAL_CONFLICT_WARNING.to_string())
        } else {
            None
        };

        DecisionReport {
            decision_score: round2(decision_score),
            weights: safe_weights,
            interpretation: interpretation_for_max_weight(safe_weights).to_string(),
            warning,
        }
    }
}

fn pair_count(n: usize) -> usize {
    n.saturating_mul(n.saturating_sub(1)) / 2
}

fn interpretation_for_max_weight(weights: DecisionWeights) -> &'static str {
    let mut max_key = "coherence";
    let mut max_value = weights.coherence;

    for (k, v) in [
        ("stability", weights.stability),
        ("conflict", weights.conflict),
        ("tradeoff", weights.tradeoff),
    ] {
        if v > max_value {
            max_key = k;
            max_value = v;
        }
    }

    match max_key {
        "coherence" => "Coherence prioritized",
        "stability" => "Stability prioritized",
        "conflict" => "Conflict minimization prioritized",
        "tradeoff" => "Tradeoff minimization prioritized",
        _ => "Coherence prioritized",
    }
}

#[cfg(test)]
mod tests {
    use semantic_dhm::{ConceptQuery, SemanticDhm};

    use super::{DecisionWeights, Recomposer};

    fn query(v0: f32, a: f32, s0: f32) -> ConceptQuery {
        let mut v = vec![0.0f32; 384];
        let mut s = vec![0.0f32; 384];
        v[0] = v0;
        s[0] = s0;
        ConceptQuery { v, a, s }
    }

    #[test]
    fn zero_weight_component_has_zero_effect() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&query(1.0, 0.2, 1.0));
        let id2 = dhm.insert_query(&query(1.0, 0.2, 1.0));
        let concepts = vec![dhm.get(id1).expect("c1"), dhm.get(id2).expect("c2")];

        let r = Recomposer;
        let out = r.decide(
            &concepts,
            DecisionWeights {
                coherence: 0.0,
                stability: 1.0,
                conflict: 0.0,
                tradeoff: 0.0,
            },
            &dhm.weights(),
        );
        assert_eq!(out.decision_score, 1.0);
    }

    #[test]
    fn increasing_weight_changes_score_monotonically() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&query(1.0, 0.2, 1.0));
        let id2 = dhm.insert_query(&query(-1.0, 0.8, -1.0));
        let concepts = vec![dhm.get(id1).expect("c1"), dhm.get(id2).expect("c2")];

        let r = Recomposer;
        let low = r.decide(
            &concepts,
            DecisionWeights {
                coherence: 0.1,
                stability: 1.0,
                conflict: 1.0,
                tradeoff: 1.0,
            },
            &dhm.weights(),
        );
        let high = r.decide(
            &concepts,
            DecisionWeights {
                coherence: 0.5,
                stability: 1.0,
                conflict: 1.0,
                tradeoff: 1.0,
            },
            &dhm.weights(),
        );
        assert!(high.decision_score <= low.decision_score);
    }

    #[test]
    fn deterministic_on_repeated_runs() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&query(1.0, 0.2, 1.0));
        let id2 = dhm.insert_query(&query(0.6, 0.4, 0.7));
        let concepts = vec![dhm.get(id1).expect("c1"), dhm.get(id2).expect("c2")];

        let r = Recomposer;
        let weights = DecisionWeights::default();
        let a = r.decide(&concepts, weights, &dhm.weights());
        let b = r.decide(&concepts, weights, &dhm.weights());
        assert_eq!(a, b);
    }

    #[test]
    fn warning_is_raised_on_structural_conflict() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&query(1.0, 0.2, 1.0));
        let id2 = dhm.insert_query(&query(-1.0, 0.2, -1.0));
        let concepts = vec![dhm.get(id1).expect("c1"), dhm.get(id2).expect("c2")];

        let r = Recomposer;
        let out = r.decide(&concepts, DecisionWeights::default(), &dhm.weights());
        assert_eq!(
            out.warning,
            Some("Decision unstable due to structural conflict".to_string())
        );
    }
}
