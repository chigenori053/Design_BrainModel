use memory_space_complex::ComplexField;
use memory_space_core::{MemoryField, MemoryId};
use memory_space_eval::evaluate_recall;
use memory_space_index::MemoryIndex;
use memory_space_recall::resonance;

use crate::query::{MemoryQuery, ScoredCandidate};

pub struct MemoryEngine<I: MemoryIndex> {
    pub memory_bank: Vec<MemoryField>,
    pub index: I,
}

impl<I: MemoryIndex> MemoryEngine<I> {
    pub fn new(index: I) -> Self {
        Self {
            memory_bank: Vec::new(),
            index,
        }
    }

    pub fn with_memory(memory_bank: Vec<MemoryField>, mut index: I) -> Self {
        for memory in &memory_bank {
            index.insert(memory.clone());
        }
        Self { memory_bank, index }
    }

    pub fn store(&mut self, memory: MemoryField) {
        self.index.insert(memory.clone());
        self.memory_bank.push(memory);
    }

    pub fn query(&self, query: MemoryQuery) -> Vec<ScoredCandidate> {
        let density = self.memory_bank.len() as f64;
        let effective_query = merge_query_with_context(&query.vector, query.context.as_ref());

        let candidate_ids = self.index.search(&effective_query, query.k);
        let mut scored = candidate_ids
            .into_iter()
            .filter_map(|memory_id| {
                let memory = self.memory_by_id(memory_id)?;
                let memory_field = ComplexField::new(memory.vector.clone());
                let resonance_value = resonance(&effective_query, &memory_field);
                let recall = evaluate_recall(resonance_value, density);
                Some(ScoredCandidate::from_parts(
                    memory_id,
                    resonance_value,
                    recall,
                ))
            })
            .collect::<Vec<_>>();

        scored.sort_by(|lhs, rhs| {
            rhs.score
                .total_cmp(&lhs.score)
                .then_with(|| rhs.resonance.total_cmp(&lhs.resonance))
                .then_with(|| lhs.memory_id.cmp(&rhs.memory_id))
        });
        scored.truncate(query.k);
        scored
    }

    fn memory_by_id(&self, memory_id: MemoryId) -> Option<&MemoryField> {
        self.memory_bank
            .iter()
            .find(|memory| memory.id == memory_id)
    }
}

fn merge_query_with_context(vector: &ComplexField, context: Option<&ComplexField>) -> ComplexField {
    match context {
        Some(ctx) => {
            let len = vector.data.len().min(ctx.data.len());
            let merged = (0..len)
                .map(|idx| vector.data[idx] + ctx.data[idx])
                .collect::<Vec<_>>();
            ComplexField::new(merged)
        }
        None => vector.clone(),
    }
}

#[cfg(test)]
mod tests {
    use memory_space_complex::{ComplexField, encode_real_vector, normalize};
    use memory_space_core::{MemoryField, MemoryId};
    use memory_space_index::LinearIndex;

    use crate::{MemoryEngine, MemoryQuery};

    fn mem(id: MemoryId, values: &[f64]) -> MemoryField {
        let mut field = encode_real_vector(values);
        normalize(&mut field);
        MemoryField {
            id,
            vector: field.data,
        }
    }

    #[test]
    fn store_updates_index_and_search() {
        let mut engine = MemoryEngine::new(LinearIndex::new());
        engine.store(mem(11, &[1.0, 0.0]));
        engine.store(mem(22, &[0.0, 1.0]));

        let mut qv = encode_real_vector(&[1.0, 0.0]);
        normalize(&mut qv);

        let out = engine.query(MemoryQuery {
            vector: qv,
            context: None,
            k: 1,
        });

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].memory_id, 11);
    }

    #[test]
    fn query_is_deterministic() {
        let engine = MemoryEngine::with_memory(
            vec![
                mem(2, &[1.0, 0.0]),
                mem(1, &[1.0, 0.0]),
                mem(3, &[0.0, 1.0]),
            ],
            LinearIndex::new(),
        );

        let mut query = encode_real_vector(&[1.0, 0.0]);
        normalize(&mut query);

        let q = MemoryQuery {
            vector: query,
            context: None,
            k: 2,
        };

        let a = engine.query(q.clone());
        let b = engine.query(q);
        assert_eq!(a, b);
        assert_eq!(a[0].memory_id, 1);
        assert_eq!(a[1].memory_id, 2);
    }

    #[test]
    fn top_hit_is_preserved() {
        let engine = MemoryEngine::with_memory(
            vec![mem(1, &[0.0, 1.0]), mem(2, &[1.0, 0.0])],
            LinearIndex::new(),
        );

        let mut query = encode_real_vector(&[1.0, 0.0]);
        normalize(&mut query);

        let out = engine.query(MemoryQuery {
            vector: query,
            context: None,
            k: 1,
        });

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].memory_id, 2);
    }

    #[test]
    fn query_uses_context_vector() {
        let engine = MemoryEngine::with_memory(
            vec![mem(1, &[1.0, 0.0]), mem(2, &[0.0, 1.0])],
            LinearIndex::new(),
        );

        let mut vector = encode_real_vector(&[0.0, 1.0]);
        normalize(&mut vector);
        let mut context = ComplexField::new(vec![
            memory_space_core::Complex64::new(1.0, 0.0),
            memory_space_core::Complex64::new(0.0, 0.0),
        ]);
        normalize(&mut context);

        let out = engine.query(MemoryQuery {
            vector,
            context: Some(context),
            k: 1,
        });

        assert_eq!(out.len(), 1);
    }
}
