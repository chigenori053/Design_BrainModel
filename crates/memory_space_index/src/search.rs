#[cfg(test)]
mod tests {
    use memory_space_complex::{encode_real_vector, normalize};
    use memory_space_core::{MemoryField, MemoryId};

    use crate::index::{LinearIndex, MemoryIndex};

    fn mem(id: MemoryId, values: &[f64]) -> MemoryField {
        let mut field = encode_real_vector(values);
        normalize(&mut field);
        MemoryField {
            id,
            vector: field.data,
        }
    }

    #[test]
    fn search_is_deterministic() {
        let mut index = LinearIndex::new();
        index.insert(mem(20, &[1.0, 0.0]));
        index.insert(mem(10, &[1.0, 0.0]));
        index.insert(mem(30, &[0.0, 1.0]));

        let mut query = encode_real_vector(&[1.0, 0.0]);
        normalize(&mut query);

        let first = index.search(&query, 3);
        let second = index.search(&query, 3);

        assert_eq!(first, second);
        assert_eq!(first[0], 10);
        assert_eq!(first[1], 20);
    }

    #[test]
    fn search_returns_expected_top_hit() {
        let index = LinearIndex::with_memory(vec![mem(1, &[0.0, 1.0]), mem(2, &[1.0, 0.0])]);

        let mut query = encode_real_vector(&[1.0, 0.0]);
        normalize(&mut query);

        let out = index.search(&query, 1);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0], 2);
    }
}
