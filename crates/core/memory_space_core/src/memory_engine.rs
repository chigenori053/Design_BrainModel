use crate::{FeatureIndex, MemoryStore, RecallConfig, RecallQuery, RecallResult};

#[derive(Debug, Clone)]
pub struct MemoryEngine<S> {
    store: S,
    index: FeatureIndex,
}

impl<S> MemoryEngine<S>
where
    S: MemoryStore,
{
    pub fn new(store: S) -> Self {
        Self {
            store,
            index: FeatureIndex,
        }
    }

    pub fn recall(&self, query: &RecallQuery, config: RecallConfig) -> RecallResult {
        let top_k = config.top_k.max(1);
        let records = self.store.query(query, top_k);
        let candidates = self
            .index
            .rank(query, &records)
            .into_iter()
            .take(top_k)
            .collect();
        RecallResult { candidates }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InMemoryMemoryStore, MemoryRecord, ModalityInput};

    #[test]
    fn recall_is_deterministic() {
        let store = InMemoryMemoryStore::with_records(vec![
            MemoryRecord {
                memory_id: 1,
                feature_vector: vec![1.0, 1.0],
                metadata: serde_json::json!({"kind":"semantic"}),
            },
            MemoryRecord {
                memory_id: 2,
                feature_vector: vec![2.0, 2.0],
                metadata: serde_json::json!({"kind":"design"}),
            },
        ]);
        let engine = MemoryEngine::new(store);
        let query = RecallQuery {
            modality: ModalityInput::Text("phase9".to_string()),
            context_vector: vec![1.0, 1.0],
            query_text: Some("phase9".to_string()),
        };

        let left = engine.recall(&query, RecallConfig { top_k: 2 });
        let right = engine.recall(&query, RecallConfig { top_k: 2 });

        assert_eq!(left, right);
        assert_eq!(left.candidates[0].memory_id, 1);
    }
}
