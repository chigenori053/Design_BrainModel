#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeuristicSignal {
    pub memory_resonance: f64,
    pub concept_match: f64,
    pub intent_alignment: f64,
}

pub fn score(signal: HeuristicSignal) -> f64 {
    (0.5 * signal.memory_resonance + 0.3 * signal.concept_match + 0.2 * signal.intent_alignment)
        .clamp(0.0, 1.0)
}
