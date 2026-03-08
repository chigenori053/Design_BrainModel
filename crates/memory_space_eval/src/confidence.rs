pub fn score_confidence(resonance: f64, ambiguity: f64) -> f64 {
    let signal = resonance.clamp(0.0, 1.0);
    let ambiguity_penalty = ambiguity.clamp(0.0, 1.0);
    (signal * (1.0 - ambiguity_penalty)).clamp(0.0, 1.0)
}
