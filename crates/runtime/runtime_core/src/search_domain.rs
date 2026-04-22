/// Domain-level search scoring — aggregates runtime signals into a score.

#[derive(Debug, Clone, Default)]
pub struct SearchInput {
    pub concept_count: usize,
    pub memory_signal: f64,
    pub intent_edges: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SearchScore {
    pub value: f64,
    pub confidence: f64,
}

pub fn compute_score(input: &SearchInput) -> SearchScore {
    let concept_signal = (input.concept_count as f64 / 10.0).clamp(0.0, 1.0);
    let memory_signal = input.memory_signal.clamp(0.0, 1.0);
    let intent_signal = (input.intent_edges as f64 / 10.0).clamp(0.0, 1.0);
    let value = (concept_signal + memory_signal + intent_signal) / 3.0;
    let confidence = if input.concept_count == 0 { 0.0 } else { 1.0 };
    SearchScore { value, confidence }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_zero_confidence() {
        let score = compute_score(&SearchInput::default());
        assert_eq!(score.confidence, 0.0);
    }

    #[test]
    fn full_signals_yield_score_near_one() {
        let input = SearchInput {
            concept_count: 10,
            memory_signal: 1.0,
            intent_edges: 10,
        };
        let score = compute_score(&input);
        assert!((score.value - 1.0).abs() < 1e-9);
        assert_eq!(score.confidence, 1.0);
    }
}
