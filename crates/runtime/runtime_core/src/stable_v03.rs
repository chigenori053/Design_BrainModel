use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use architecture_ir::stable_v03::ArchitectureGraph;
use design_search_engine::stable_v03::{
    ArchitectureCandidate, Constraint, DesignSearchEngine, RecallContext, RecalledPattern,
    SearchInput,
};
use memory_space_phase14::stable_v03::{MemoryEngine, MemoryRecord, RecallInput};
use world_model::stable_v03::{IntentInput, IntentState};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CoreError {
    InvalidInput,
    SearchFailed,
    MemoryError,
}

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Clone, Debug, PartialEq)]
pub struct ExecutionTrace {
    pub recall_used: bool,
    pub candidate_count: usize,
    pub selected_score: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeResult {
    pub architecture: ArchitectureGraph,
    pub trace: ExecutionTrace,
}

pub struct CoreRuntime {
    pub executor: RuntimeExecutor,
}

impl CoreRuntime {
    pub fn new(memory: Arc<dyn MemoryEngine>, search: Arc<dyn DesignSearchEngine>) -> Self {
        Self {
            executor: RuntimeExecutor { memory, search },
        }
    }
}

pub struct RuntimeExecutor {
    memory: Arc<dyn MemoryEngine>,
    search: Arc<dyn DesignSearchEngine>,
}

impl RuntimeExecutor {
    pub fn execute(&self, input: IntentInput) -> CoreResult<RuntimeResult> {
        let intent = parse(input)?;
        let recall_result = self.memory.recall(RecallInput {
            intent: intent.clone(),
            limit: 5,
        });
        let recall = to_recall_context(&intent, &recall_result);
        let candidates = self.search.search(SearchInput {
            intent: intent.clone(),
            recall: recall.clone(),
        });
        let candidate_count = candidates.len();
        let selected = select_best(candidates).ok_or(CoreError::SearchFailed)?;

        self.memory.store(MemoryRecord {
            id: stable_id(&format!("{}:{}", intent.raw, selected.id)),
            text: intent.raw,
            tags: intent.tokens,
            embedding: None,
            architecture: Some(selected.architecture.clone()),
            relations: vec!["selected".to_string()],
        });

        Ok(RuntimeResult {
            architecture: selected.architecture,
            trace: ExecutionTrace {
                recall_used: recall.is_some(),
                candidate_count,
                selected_score: selected.score,
            },
        })
    }
}

fn parse(input: IntentInput) -> CoreResult<IntentState> {
    let tokens = input
        .raw
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Err(CoreError::InvalidInput);
    }
    Ok(IntentState {
        raw: input.raw,
        tokens,
    })
}

fn to_recall_context(
    intent: &IntentState,
    recall: &memory_space_phase14::stable_v03::RecallResult,
) -> Option<RecallContext> {
    if recall.records.is_empty() {
        return None;
    }
    Some(RecallContext {
        patterns: recall
            .records
            .iter()
            .filter_map(|record| {
                record
                    .record
                    .architecture
                    .clone()
                    .map(|architecture| RecalledPattern {
                        record_id: record.record.id.clone(),
                        architecture,
                        score: record.score,
                        tags: record.record.tags.clone(),
                    })
            })
            .collect(),
        constraints: intent
            .tokens
            .iter()
            .filter(|token| token.contains("must") || token.contains("only"))
            .map(|token| Constraint {
                key: "intent".to_string(),
                value: token.clone(),
            })
            .collect(),
        confidence: recall.confidence,
    })
}

fn select_best(candidates: Vec<ArchitectureCandidate>) -> Option<ArchitectureCandidate> {
    candidates.into_iter().next()
}

fn stable_id(value: &str) -> String {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    format!("intent-{:016x}", hasher.finish())
}
