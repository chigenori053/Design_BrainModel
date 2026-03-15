use crate::SearchState;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchTelemetry {
    pub explored_states: usize,
    pub generated_states: usize,
    pub pruned_states: usize,
    pub pareto_states: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchOutcome {
    pub states: Vec<SearchState>,
    pub telemetry: SearchTelemetry,
}
