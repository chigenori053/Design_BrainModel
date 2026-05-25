use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::RwLock;

use architecture_ir::stable_v03::ArchitectureGraph;
pub use contracts::{MemoryCandidate, MemoryId, MemorySource};
use world_model::stable_v03::IntentState;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecallInput {
    pub intent: IntentState,
    pub limit: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecallConfig {
    pub top_k: usize,
    pub threshold: f64,
}

impl Default for RecallConfig {
    fn default() -> Self {
        Self {
            top_k: 5,
            threshold: 0.1,
        }
    }
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: usize,
    pub misses: usize,
    pub evictions: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryNode {
    pub id: String,
    pub embedding: Vec<u32>,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MemoryRelation {
    Causal,
    Sequential,
    Similarity,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemoryEdge {
    pub from: String,
    pub to: String,
    pub relation: MemoryRelation,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemoryGraphSnapshot {
    pub nodes: Vec<MemoryNode>,
    pub edges: Vec<MemoryEdge>,
}

pub trait MemoryEngine: Send + Sync {
    fn recall(&self, input: RecallInput) -> RecallResult;
    fn retrieve(&self, query: MemoryQuery) -> Vec<MemoryRecord>;
    fn store(&self, record: MemoryRecord);
}

#[derive(Debug, Default)]
pub struct InMemoryEngine {
    records: RwLock<Vec<MemoryRecord>>,
    recall_cache: RwLock<BTreeMap<String, RecallResult>>,
    cache_order: RwLock<VecDeque<String>>,
    cache_stats: RwLock<CacheStats>,
    edges: RwLock<Vec<MemoryEdge>>,
}

impl InMemoryEngine {
    pub fn records(&self) -> Vec<MemoryRecord> {
        self.records.read().expect("memory read lock").clone()
    }

    pub fn recall_candidates(
        &self,
        input: RecallInput,
        config: RecallConfig,
    ) -> Vec<MemoryCandidate> {
        self.recall(input)
            .records
            .into_iter()
            .filter(|record| record.score >= config.threshold)
            .take(config.top_k.max(1))
            .enumerate()
            .map(|(rank, record)| MemoryCandidate {
                id: record.record.id,
                score: record.score as f32,
                source: MemorySource::Exact,
                rank,
            })
            .collect()
    }

    pub fn store_edge(
        &self,
        from: impl Into<String>,
        to: impl Into<String>,
        relation: MemoryRelation,
    ) {
        let mut edges = self.edges.write().expect("memory edge write lock");
        let edge = MemoryEdge {
            from: from.into(),
            to: to.into(),
            relation,
        };
        if !edges.contains(&edge) {
            edges.push(edge);
            edges.sort_by(|lhs, rhs| lhs.from.cmp(&rhs.from).then_with(|| lhs.to.cmp(&rhs.to)));
        }
    }

    pub fn graph_snapshot(&self) -> MemoryGraphSnapshot {
        let records = self.records();
        let node_ids = self
            .edges
            .read()
            .expect("memory edge read lock")
            .iter()
            .flat_map(|edge| [edge.from.clone(), edge.to.clone()])
            .collect::<BTreeSet<_>>();
        let mut nodes = records
            .into_iter()
            .map(|record| MemoryNode {
                id: record.id,
                embedding: record
                    .embedding
                    .unwrap_or_default()
                    .into_iter()
                    .map(|value| value.to_bits())
                    .collect(),
                metadata: BTreeMap::from([
                    ("text".to_string(), record.text),
                    ("tags".to_string(), record.tags.join(",")),
                ]),
            })
            .collect::<Vec<_>>();
        for node_id in node_ids {
            if nodes.iter().any(|node| node.id == node_id) {
                continue;
            }
            nodes.push(MemoryNode {
                id: node_id,
                embedding: Vec::new(),
                metadata: BTreeMap::new(),
            });
        }
        nodes.sort_by(|lhs, rhs| lhs.id.cmp(&rhs.id));
        MemoryGraphSnapshot {
            nodes,
            edges: self.edges.read().expect("memory edge read lock").clone(),
        }
    }

    pub fn cache_stats(&self) -> CacheStats {
        self.cache_stats
            .read()
            .expect("cache stats read lock")
            .clone()
    }
}

impl MemoryEngine for InMemoryEngine {
    fn recall(&self, input: RecallInput) -> RecallResult {
        let cache_key = format!(
            "{}::{}",
            input.intent.raw.to_ascii_lowercase(),
            input.limit.max(1)
        );
        if let Some(cached) = self
            .recall_cache
            .read()
            .expect("recall cache read lock")
            .get(&cache_key)
            .cloned()
        {
            self.bump_cache_hit(&cache_key);
            return cached;
        }
        self.cache_stats
            .write()
            .expect("cache stats write lock")
            .misses += 1;
        let query_terms = normalized_terms(&input.intent.raw, &input.intent.tokens);
        let approx_limit = (input.limit.max(1) * 3).max(6);
        let approximate = self.retrieve_approximate(&query_terms, approx_limit);
        let mut recalled = approximate
            .into_iter()
            .map(|record| RecalledRecord {
                score: score_record(&query_terms, &record),
                record,
            })
            .collect::<Vec<_>>();
        normalize_scores(&mut recalled);
        recalled.retain(|record| record.score >= 0.1);
        prioritize_cluster_neighbors(
            &mut recalled,
            &self.edges.read().expect("memory edge read lock"),
        );
        recalled.sort_by(|lhs, rhs| {
            rhs.score
                .total_cmp(&lhs.score)
                .then_with(|| lhs.record.id.cmp(&rhs.record.id))
        });
        recalled.truncate(input.limit.max(1));
        let confidence = if recalled.is_empty() {
            0.0
        } else {
            recalled.iter().map(|record| record.score).sum::<f64>() / recalled.len() as f64
        };
        RecallResult {
            records: recalled,
            confidence,
        }
        .tap(|result| {
            self.insert_cache_entry(cache_key, result.clone());
        })
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
        self.recall_cache
            .write()
            .expect("recall cache write lock")
            .clear();
        self.cache_order
            .write()
            .expect("cache order write lock")
            .clear();
    }
}

impl InMemoryEngine {
    fn retrieve_approximate(&self, query_terms: &[String], limit: usize) -> Vec<MemoryRecord> {
        let mut scored = self
            .records
            .read()
            .expect("memory read lock")
            .iter()
            .filter_map(|record| {
                let record_terms = normalized_terms(&record.text, &record.tags);
                let approx = approximate_score(query_terms, &record_terms);
                (approx > 0.0).then_some((record.clone(), approx))
            })
            .collect::<Vec<_>>();
        scored.sort_by(|lhs, rhs| {
            rhs.1
                .total_cmp(&lhs.1)
                .then_with(|| lhs.0.id.cmp(&rhs.0.id))
        });
        scored
            .into_iter()
            .take(limit.max(1))
            .map(|(record, _)| record)
            .collect()
    }

    fn insert_cache_entry(&self, cache_key: String, result: RecallResult) {
        const MAX_CACHE_ENTRIES: usize = 32;
        self.recall_cache
            .write()
            .expect("recall cache write lock")
            .insert(cache_key.clone(), result);
        let mut order = self.cache_order.write().expect("cache order write lock");
        if let Some(index) = order.iter().position(|entry| entry == &cache_key) {
            order.remove(index);
        }
        order.push_back(cache_key.clone());
        while order.len() > MAX_CACHE_ENTRIES {
            if let Some(evicted) = order.pop_front() {
                self.recall_cache
                    .write()
                    .expect("recall cache write lock")
                    .remove(&evicted);
                self.cache_stats
                    .write()
                    .expect("cache stats write lock")
                    .evictions += 1;
            }
        }
    }

    fn bump_cache_hit(&self, cache_key: &str) {
        self.cache_stats
            .write()
            .expect("cache stats write lock")
            .hits += 1;
        let mut order = self.cache_order.write().expect("cache order write lock");
        if let Some(index) = order.iter().position(|entry| entry == cache_key) {
            let entry = order.remove(index).expect("cache order entry should exist");
            order.push_back(entry);
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

fn approximate_score(query_terms: &[String], record_terms: &[String]) -> f64 {
    if query_terms.is_empty() || record_terms.is_empty() {
        return 0.0;
    }
    let overlap = query_terms
        .iter()
        .filter(|term| record_terms.binary_search(term).is_ok())
        .count() as f64;
    overlap / query_terms.len().max(record_terms.len()) as f64
}

fn normalize_scores(records: &mut [RecalledRecord]) {
    let max_score = records
        .iter()
        .map(|record| record.score)
        .fold(0.0_f64, f64::max);
    if max_score <= f64::EPSILON {
        return;
    }
    for record in records {
        record.score /= max_score;
    }
}

fn prioritize_cluster_neighbors(records: &mut [RecalledRecord], edges: &[MemoryEdge]) {
    let top_id = records.first().map(|record| record.record.id.clone());
    let Some(top_id) = top_id else {
        return;
    };
    let boosted = edges
        .iter()
        .filter(|edge| edge.from == top_id || edge.to == top_id)
        .flat_map(|edge| [edge.from.clone(), edge.to.clone()])
        .collect::<BTreeSet<_>>();
    for record in records {
        if boosted.contains(&record.record.id) {
            record.score = (record.score + 0.1).clamp(0.0, 1.0);
        }
    }
}

trait Tap: Sized {
    fn tap(self, apply: impl FnOnce(&Self)) -> Self {
        apply(&self);
        self
    }
}

impl<T> Tap for T {}
