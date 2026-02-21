use semantic_dhm::ConceptUnit;

use crate::Recomposer;
use crate::consistency::compute_consistency;
use crate::explain::round2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecisionWeights {
    pub coherence: f32,
    pub stability: f32,
    pub conflict: f32,
    pub tradeoff: f32,
}

impl Default for DecisionWeights {
    fn default() -> Self {
        Self {
            coherence: 0.40,
            stability: 0.30,
            conflict: 0.20,
            tradeoff: 0.10,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecisionReport {
    pub decision_score: f32,
    pub interpretation: String,
    pub warning: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecisionError {
    InvalidWeightConfiguration,
}

impl std::fmt::Display for DecisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidWeightConfiguration => write!(f, "invalid weight configuration"),
        }
    }
}

impl std::error::Error for DecisionError {}

impl DecisionWeights {
    pub fn normalized(self) -> Result<Self, DecisionError> {
        let clipped = Self {
            coherence: self.coherence.max(0.0),
            stability: self.stability.max(0.0),
            conflict: self.conflict.max(0.0),
            tradeoff: self.tradeoff.max(0.0),
        };
        let sum = clipped.coherence + clipped.stability + clipped.conflict + clipped.tradeoff;
        if sum <= f32::EPSILON {
            return Err(DecisionError::InvalidWeightConfiguration);
        }
        Ok(Self {
            coherence: clipped.coherence / sum,
            stability: clipped.stability / sum,
            conflict: clipped.conflict / sum,
            tradeoff: clipped.tradeoff / sum,
        })
    }
}

impl Recomposer {
    pub fn decide(
        &self,
        concepts: &[ConceptUnit],
        weights: DecisionWeights,
        resonance_weights: &semantic_dhm::ResonanceWeights,
    ) -> Result<DecisionReport, DecisionError> {
        let w = weights.normalized()?;
        let consistency = compute_consistency(concepts, resonance_weights);

        let pair_count = concepts
            .len()
            .saturating_mul(concepts.len().saturating_sub(1))
            / 2;
        let coherence_score = ((consistency.global_coherence + 1.0) * 0.5).clamp(0.0, 1.0);
        let stability_score = consistency.report.stability_score.clamp(0.0, 1.0);
        let conflict_score = if pair_count == 0 {
            1.0
        } else {
            1.0 - (consistency.report.structural_conflicts as f32 / pair_count as f32)
        }
        .clamp(0.0, 1.0);
        let tradeoff_score = if consistency.report.tradeoffs.is_empty() {
            1.0
        } else {
            let mean_tension = consistency
                .report
                .tradeoffs
                .iter()
                .map(|t| t.tension)
                .sum::<f32>()
                / consistency.report.tradeoffs.len() as f32;
            (1.0 - mean_tension).clamp(0.0, 1.0)
        };

        let decision_score = round2(
            w.coherence * coherence_score
                + w.stability * stability_score
                + w.conflict * conflict_score
                + w.tradeoff * tradeoff_score,
        );
        let interpretation = if decision_score >= 0.70 {
            "Design is structurally consistent."
        } else if decision_score >= 0.40 {
            "Design needs refinement."
        } else {
            "Design has major structural issues."
        }
        .to_string();
        let warning = if consistency.report.structural_conflicts > 0 {
            Some("Decision unstable due to structural conflict.".to_string())
        } else {
            None
        };

        Ok(DecisionReport {
            decision_score,
            interpretation,
            warning,
        })
    }
}

#[cfg(test)]
mod tests {
    use semantic_dhm::{ConceptQuery, SemanticDhm};

    use crate::{DecisionError, DecisionWeights, Recomposer};

    fn query(v0: f32, v1: f32, a: f32, s0: f32, s1: f32, polarity: i8) -> ConceptQuery {
        let mut v = vec![0.0f32; 384];
        let mut s = vec![0.0f32; 384];
        v[0] = v0;
        v[1] = v1;
        s[0] = s0;
        s[1] = s1;
        ConceptQuery { v, a, s, polarity }
    }

    #[test]
    fn zero_weight_sum_is_error() {
        let out = DecisionWeights {
            coherence: 0.0,
            stability: 0.0,
            conflict: 0.0,
            tradeoff: 0.0,
        }
        .normalized();
        assert_eq!(out, Err(DecisionError::InvalidWeightConfiguration));
    }

    #[test]
    fn warning_is_emitted_on_structural_conflict() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&query(1.0, 0.0, 0.20, 1.0, 0.0, -1));
        let id2 = dhm.insert_query(&query(1.0, 0.0, 0.20, 1.0, 0.0, 1));
        let c1 = dhm.get(id1).expect("c1");
        let c2 = dhm.get(id2).expect("c2");

        let out = Recomposer
            .decide(&[c1, c2], DecisionWeights::default(), &dhm.weights())
            .expect("decide");
        assert_eq!(
            out.warning.as_deref(),
            Some("Decision unstable due to structural conflict.")
        );
    }

    #[test]
    fn score_changes_when_weights_change() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&query(1.0, 0.0, 0.10, 1.0, 0.0, 0));
        let id2 = dhm.insert_query(&query(1.0, 0.0, 0.90, 1.0, 0.0, 0));
        let id3 = dhm.insert_query(&query(0.2, 0.9, 0.50, 0.2, 0.9, 0));
        let concepts = vec![
            dhm.get(id1).expect("c1"),
            dhm.get(id2).expect("c2"),
            dhm.get(id3).expect("c3"),
        ];

        let a = Recomposer
            .decide(
                &concepts,
                DecisionWeights {
                    coherence: 0.7,
                    stability: 0.1,
                    conflict: 0.1,
                    tradeoff: 0.1,
                },
                &dhm.weights(),
            )
            .expect("a");
        let b = Recomposer
            .decide(
                &concepts,
                DecisionWeights {
                    coherence: 0.1,
                    stability: 0.1,
                    conflict: 0.7,
                    tradeoff: 0.1,
                },
                &dhm.weights(),
            )
            .expect("b");
        assert_ne!(a.decision_score, b.decision_score);
    }

    #[test]
    fn deterministic_for_same_input() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&query(1.0, 0.0, 0.20, 1.0, 0.0, 0));
        let id2 = dhm.insert_query(&query(0.8, 0.2, 0.30, 0.9, 0.1, 0));
        let concepts = vec![dhm.get(id1).expect("c1"), dhm.get(id2).expect("c2")];

        let w = DecisionWeights::default();
        let first = Recomposer
            .decide(&concepts, w, &dhm.weights())
            .expect("first");
        let second = Recomposer
            .decide(&concepts, w, &dhm.weights())
            .expect("second");
        assert_eq!(first, second);
    }

    #[test]
    fn conflict_weight_monotonicity() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let id1 = dhm.insert_query(&query(1.0, 0.0, 0.20, 1.0, 0.0, -1));
        let id2 = dhm.insert_query(&query(1.0, 0.0, 0.20, 1.0, 0.0, 1));
        let concepts = vec![dhm.get(id1).expect("c1"), dhm.get(id2).expect("c2")];

        let low = Recomposer
            .decide(
                &concepts,
                DecisionWeights {
                    coherence: 0.8,
                    stability: 0.1,
                    conflict: 0.05,
                    tradeoff: 0.05,
                },
                &dhm.weights(),
            )
            .expect("low");
        let high = Recomposer
            .decide(
                &concepts,
                DecisionWeights {
                    coherence: 0.1,
                    stability: 0.1,
                    conflict: 0.7,
                    tradeoff: 0.1,
                },
                &dhm.weights(),
            )
            .expect("high");
        assert!(high.decision_score < low.decision_score);
    }
}
