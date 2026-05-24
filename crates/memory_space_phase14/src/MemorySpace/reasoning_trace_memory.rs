use std::collections::BTreeMap;

use super::types::DesignIntentRecord;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchStep {
    pub step_id: usize,
    pub action: String,
    pub score: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReasoningTrace {
    pub trace_id: String,
    pub intent: DesignIntentRecord,
    pub selected_template: String,
    pub search_steps: Vec<SearchStep>,
    pub candidate_architectures: Vec<String>,
    pub final_architecture: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReasoningTraceMemoryDomain {
    records: BTreeMap<String, ReasoningTrace>,
}

impl ReasoningTraceMemoryDomain {
    pub fn upsert(&mut self, record: ReasoningTrace) {
        self.records.insert(record.trace_id.clone(), record);
    }

    pub fn get(&self, trace_id: &str) -> Option<&ReasoningTrace> {
        self.records.get(trace_id)
    }

    pub fn all(&self) -> Vec<&ReasoningTrace> {
        self.records.values().collect()
    }
}
