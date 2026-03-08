use memory_space_complex::ComplexField;
use memory_space_core::{MemoryCandidate, MemoryField};

use crate::{rank_candidates, resonance};

pub fn recall_top_k(
    query: &ComplexField,
    memory_bank: &[MemoryField],
    k: usize,
) -> Vec<MemoryCandidate> {
    let candidates = memory_bank
        .iter()
        .map(|memory| {
            let field = ComplexField::new(memory.vector.clone());
            MemoryCandidate {
                memory_id: memory.id,
                resonance: resonance(query, &field),
            }
        })
        .collect();

    rank_candidates(candidates, k)
}

#[cfg(test)]
mod tests {
    use memory_space_complex::{encode_real_vector, normalize};
    use memory_space_core::{MemoryField, MemoryId};

    use crate::recall_top_k;

    fn mem(id: MemoryId, values: &[f64]) -> MemoryField {
        let mut field = encode_real_vector(values);
        normalize(&mut field);
        MemoryField {
            id,
            vector: field.data,
        }
    }

    #[test]
    fn deterministic_ranking_with_tie_break() {
        let mut query = encode_real_vector(&[1.0, 0.0]);
        normalize(&mut query);

        let bank = vec![
            mem(20, &[1.0, 0.0]),
            mem(10, &[1.0, 0.0]),
            mem(30, &[0.0, 1.0]),
        ];

        let first = recall_top_k(&query, &bank, 3);
        let second = recall_top_k(&query, &bank, 3);

        assert_eq!(first, second);
        assert_eq!(first[0].memory_id, 10);
        assert_eq!(first[1].memory_id, 20);
    }

    #[test]
    fn resonance_is_bounded() {
        let mut query = encode_real_vector(&[1.0, 2.0, 3.0]);
        normalize(&mut query);

        let bank = vec![mem(1, &[3.0, 2.0, 1.0])];
        let out = recall_top_k(&query, &bank, 1);
        assert!(out[0].resonance >= 0.0);
        assert!(out[0].resonance <= 1.0);
    }
}
