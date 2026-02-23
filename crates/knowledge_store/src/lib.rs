use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum FeedbackAction {
    Adopt,
    Reject,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeedbackEntry {
    pub context_hash: u64,
    pub applied_pattern_id: String,
    pub action: FeedbackAction,
    pub timestamp: u64,
}

#[derive(Clone, Debug, Default)]
pub struct KnowledgeStore {
    memory: Vec<Vec<f32>>,
    labels: Vec<String>,
    relevance_weights: HashMap<String, f32>,
    feedback_history: Vec<FeedbackEntry>,
}

impl KnowledgeStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_knowledge(&mut self, topic: &str, vector: Vec<f32>) {
        self.labels.push(topic.to_string());
        self.memory.push(vector);
        self.relevance_weights.entry(topic.to_string()).or_insert(1.0);
    }

    pub fn preload_defaults(&mut self) {
        if !self.labels.is_empty() {
            return;
        }
        self.add_knowledge("ユーザー認証", vec![0.8, 0.1, 0.2, 0.0]);
        self.add_knowledge("権限管理", vec![0.7, 0.2, 0.3, 0.1]);
        self.add_knowledge("データベース永続化", vec![0.1, 0.9, 0.2, 0.2]);
        self.add_knowledge("監査ログ", vec![0.4, 0.3, 0.9, 0.1]);
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }

    pub fn top_related_labels(&self, query: &[f32], top_k: usize) -> Vec<String> {
        if top_k == 0 || self.labels.is_empty() {
            return Vec::new();
        }
        let mut scored = self
            .memory
            .iter()
            .enumerate()
            .map(|(i, v)| {
                let label = &self.labels[i];
                let weight = self.relevance_weights.get(label).copied().unwrap_or(1.0);
                (i, cosine_similarity(query, v) * weight)
            })
            .collect::<Vec<_>>();
        scored.sort_by(|(_, l), (_, r)| r.total_cmp(l));
        scored
            .into_iter()
            .take(top_k.min(self.labels.len()))
            .map(|(idx, _)| self.labels[idx].clone())
            .collect()
    }

    pub fn record_feedback(&mut self, draft_id: &str, action: FeedbackAction) {
        let entry = FeedbackEntry {
            context_hash: hash_context(draft_id),
            applied_pattern_id: pattern_from_draft_id(draft_id).to_string(),
            action,
            timestamp: now_epoch_seconds(),
        };
        self.feedback_history.push(entry);
    }

    pub fn adjust_weights(&mut self) {
        for label in &self.labels {
            self.relevance_weights.insert(label.clone(), 1.0);
        }
        for entry in &self.feedback_history {
            let current = self
                .relevance_weights
                .get(&entry.applied_pattern_id)
                .copied()
                .unwrap_or(1.0);
            let next = match entry.action {
                FeedbackAction::Adopt => current + 0.10,
                FeedbackAction::Reject => current - 0.20,
            }
            .clamp(0.10, 3.0);
            self.relevance_weights
                .insert(entry.applied_pattern_id.clone(), next);
        }
    }

    pub fn feedback_entries(&self) -> &[FeedbackEntry] {
        &self.feedback_history
    }

    pub fn load_feedback_entries(&mut self, entries: Vec<FeedbackEntry>) {
        self.feedback_history = entries;
        self.adjust_weights();
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..n {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na <= 1e-12 || nb <= 1e-12 {
        0.0
    } else {
        dot / (na.sqrt() * nb.sqrt())
    }
}

fn pattern_from_draft_id(draft_id: &str) -> &str {
    if let Some((_, suffix)) = draft_id.rsplit_once('-') {
        suffix
    } else {
        draft_id
    }
}

fn hash_context(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn now_epoch_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
