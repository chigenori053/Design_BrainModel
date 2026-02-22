use crate::domain::AgentEvent;

pub fn execute_trace(config: crate::TraceRunConfig) -> Vec<crate::TraceRow> {
    execute_trace_baseline_off(config)
}

pub fn execute_trace_baseline_off(config: crate::TraceRunConfig) -> Vec<crate::TraceRow> {
    let result = crate::capability::execute_baseline_off_core(config);
    apply_events(&result.events);
    result.trace
}

pub fn execute_trace_baseline_off_balanced(
    config: crate::TraceRunConfig,
    m: usize,
) -> Vec<crate::TraceRow> {
    let result = crate::capability::execute_balanced_core(config, m);
    apply_events(&result.events);
    result.trace
}

fn apply_events(events: &[AgentEvent]) {
    for event in events {
        if let AgentEvent::WriteRawObjectives {
            path,
            depth,
            objectives,
        } = event
        {
            let _ = crate::adapters::file_storage::append_raw_objectives(path, *depth, objectives);
        }
    }
}
