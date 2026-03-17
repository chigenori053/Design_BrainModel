use std::sync::RwLock;

use architecture_ir::stable_v03::ArchitectureGraph;
use world_model::stable_v03::IntentState;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecallInput {
    pub intent: IntentState,
    pub limit: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryQuery {
    pub text: String,
    pub tags: Vec<String>,
    pub limit: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryRecord {
    pub id: String,
    pub text: String,
    pub tags: Vec<String>,
    pub embedding: Option<Vec<f32>>,
    pub architecture: Option<ArchitectureGraph>,
    pub relations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecalledRecord {
    pub record: MemoryRecord,
    pub score: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecallResult {
    pub records: Vec<RecalledRecord>,
    pub confidence: f64,
}

pub trait MemoryEngine: Send + Sync {
    fn recall(&self, input: RecallInput) -> RecallResult;
    fn retrieve(&self, query: MemoryQuery) -> Vec<MemoryRecord>;
    fn store(&self, record: MemoryRecord);
}

#[derive(Debug, Default)]
pub struct InMemoryEngine {
    records: RwLock<Vec<MemoryRecord>>,
}

impl InMemoryEngine {
    pub fn records(&self) -> Vec<MemoryRecord> {
        self.records.read().expect("memory read lock").clone()
    }
}

impl MemoryEngine for InMemoryEngine {
    fn recall(&self, input: RecallInput) -> RecallResult {
        let records = self.retrieve(MemoryQuery {
            text: input.intent.raw,
            tags: input.intent.tokens,
            limit: input.limit,
        });
        let recalled = records
            .into_iter()
            .map(|record| RecalledRecord {
                score: score_record(&normalized_terms(&record.text, &record.tags), &record),
                record,
            })
            .collect::<Vec<_>>();
        let confidence = if recalled.is_empty() {
            0.0
        } else {
            recalled.iter().map(|record| record.score).sum::<f64>() / recalled.len() as f64
        };
        RecallResult {
            records: recalled,
            confidence,
        }
    }

    fn retrieve(&self, query: MemoryQuery) -> Vec<MemoryRecord> {
        let query_terms = normalized_terms(&query.text, &query.tags);
        let mut results = self
            .records
            .read()
            .expect("memory read lock")
            .iter()
            .filter(|record| score_record(&query_terms, record) > 0.0)
            .cloned()
            .collect::<Vec<_>>();
        results.sort_by(|lhs, rhs| {
            score_record(&query_terms, rhs)
                .total_cmp(&score_record(&query_terms, lhs))
                .then_with(|| lhs.id.cmp(&rhs.id))
        });
        results.truncate(query.limit.max(1));
        results
    }

    fn store(&self, record: MemoryRecord) {
        let mut records = self.records.write().expect("memory write lock");
        if let Some(existing) = records
            .iter_mut()
            .find(|candidate| candidate.id == record.id)
        {
            *existing = record;
        } else {
            records.push(record);
            records.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
        }
    }
}

fn normalized_terms(text: &str, tags: &[String]) -> Vec<String> {
    let mut terms = text
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|term| !term.is_empty())
        .map(|term| term.to_ascii_lowercase())
        .collect::<Vec<_>>();
    terms.extend(tags.iter().map(|tag| tag.to_ascii_lowercase()));
    terms.sort();
    terms.dedup();
    terms
}

fn score_record(query_terms: &[String], record: &MemoryRecord) -> f64 {
    let record_terms = normalized_terms(&record.text, &record.tags);
    let overlap = query_terms
        .iter()
        .filter(|term| record_terms.binary_search(term).is_ok())
        .count() as f64;
    if overlap == 0.0 {
        return 0.0;
    }
    let embedding_bonus = record
        .embedding
        .as_ref()
        .map(|value| value.len() as f64)
        .unwrap_or(0.0)
        / 100.0;
    let graph_bonus = if record.architecture.is_some() {
        0.1
    } else {
        0.0
    };
    overlap + embedding_bonus + graph_bonus
}
