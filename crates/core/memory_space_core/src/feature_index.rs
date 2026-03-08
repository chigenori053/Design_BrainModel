use crate::{MemoryRecord, RecallCandidate, RecallQuery};

#[derive(Debug, Clone, Default)]
pub struct FeatureIndex;

impl FeatureIndex {
    pub fn rank(&self, query: &RecallQuery, records: &[MemoryRecord]) -> Vec<RecallCandidate> {
        let mut candidates = records
            .iter()
            .map(|record| RecallCandidate {
                memory_id: record.memory_id,
                feature_vector: record.feature_vector.clone(),
                relevance_score: score(query, record),
            })
            .collect::<Vec<_>>();

        candidates.sort_by(|lhs, rhs| {
            rhs.relevance_score
                .total_cmp(&lhs.relevance_score)
                .then_with(|| lhs.memory_id.cmp(&rhs.memory_id))
        });
        candidates
    }
}

fn score(query: &RecallQuery, record: &MemoryRecord) -> f64 {
    let dims = query.context_vector.len().min(record.feature_vector.len());
    if dims == 0 {
        return 0.0;
    }

    let total_delta = query
        .context_vector
        .iter()
        .zip(record.feature_vector.iter())
        .take(dims)
        .map(|(lhs, rhs)| (lhs - rhs).abs())
        .sum::<f64>();
    let mean_delta = total_delta / dims as f64;

    (1.0 / (1.0 + mean_delta)).clamp(0.0, 1.0)
}
