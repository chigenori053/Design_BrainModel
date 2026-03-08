use concept_engine::ConceptId;
use memory_space_complex::{ComplexField, bind, normalize};

use crate::types::Hypothesis;

pub fn generate_hypotheses(state: &ComplexField) -> Vec<Hypothesis> {
    generate_hypotheses_with_context(state, None, 3)
}

pub fn generate_hypotheses_with_context(
    state: &ComplexField,
    context: Option<&ComplexField>,
    max_hypotheses: usize,
) -> Vec<Hypothesis> {
    let mut normalized = state.clone();
    normalize(&mut normalized);

    let mut candidates = vec![
        Hypothesis {
            action_vector: state.clone(),
            predicted_score: 0.60,
        },
        Hypothesis {
            action_vector: normalized.clone(),
            predicted_score: 0.55,
        },
    ];

    let context_bound = match context {
        Some(ctx) => bind(state, ctx),
        None => bind(state, &normalized),
    };
    candidates.push(Hypothesis {
        action_vector: context_bound,
        predicted_score: 0.50,
    });

    candidates.truncate(max_hypotheses.max(1));
    candidates
}

pub fn generate_bound_concept_pairs(
    concepts: &[ConceptId],
    max_pairs: usize,
) -> Vec<(ConceptId, ConceptId)> {
    if max_pairs == 0 || concepts.len() < 2 {
        return Vec::new();
    }

    let mut sorted = concepts.to_vec();
    sorted.sort_by_key(|concept| concept.0);
    sorted.dedup();

    let mut out = Vec::new();
    for i in 0..sorted.len() {
        for j in (i + 1)..sorted.len() {
            out.push((sorted[i], sorted[j]));
            if out.len() >= max_pairs {
                return out;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use concept_engine::ConceptId;

    use super::generate_bound_concept_pairs;

    #[test]
    fn concept_pair_generation_is_unique_and_ordered() {
        let c = [
            ConceptId::from_name("cache"),
            ConceptId::from_name("database"),
            ConceptId::from_name("cache"),
            ConceptId::from_name("network"),
        ];

        let pairs = generate_bound_concept_pairs(&c, 10);
        assert_eq!(pairs.len(), 3);
        assert!(pairs[0].0.0 < pairs[0].1.0);
    }
}
