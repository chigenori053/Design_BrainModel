use std::collections::HashMap;

use crate::ConceptId;

pub fn top_k_activation(scores: &HashMap<ConceptId, f32>, top_k: usize) -> Vec<(ConceptId, f32)> {
    let mut out = scores
        .iter()
        .map(|(id, score)| (*id, *score))
        .collect::<Vec<_>>();

    out.sort_by(|lhs, rhs| rhs.1.total_cmp(&lhs.1).then_with(|| lhs.0.0.cmp(&rhs.0.0)));
    out.truncate(top_k);
    out
}
