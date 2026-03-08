use crate::{ambiguity::estimate_ambiguity, confidence::score_confidence};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecallScore {
    pub score: f64,
    pub confidence: f64,
    pub ambiguity: f64,
}

pub fn evaluate_recall(resonance: f64, memory_density: f64) -> RecallScore {
    let bounded_resonance = resonance.clamp(0.0, 1.0);
    let ambiguity = estimate_ambiguity(bounded_resonance, memory_density);
    let confidence = score_confidence(bounded_resonance, ambiguity);
    let score = (0.7 * bounded_resonance + 0.3 * confidence).clamp(0.0, 1.0);

    RecallScore {
        score,
        confidence,
        ambiguity,
    }
}
