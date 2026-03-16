use std::time::Duration;

use crate::SearchState;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchTelemetry {
    pub search_depth: usize,
    pub candidate_count: usize,
    pub explored_states: usize,
    pub generated_states: usize,
    pub pruned_states: usize,
    pub pareto_states: usize,
    pub evaluation_time_ms: u64,
    pub timeout_reached: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchOutcome {
    pub states: Vec<SearchState>,
    pub telemetry: SearchTelemetry,
}

impl SearchTelemetry {
    pub fn record_evaluation_time(&mut self, elapsed: Duration) {
        self.evaluation_time_ms = self
            .evaluation_time_ms
            .saturating_add(elapsed.as_millis().try_into().unwrap_or(u64::MAX));
    }
}
