use std::path::PathBuf;

pub use crate::core::{CoreEvent, CoreExecutor, CoreRequest, RuntimeCoreBridge};
use crate::pipeline::PipelineState;

use super::state::{EventQueue, TuiState, UiEvent};

pub fn to_ui_event(event: CoreEvent) -> UiEvent {
    match event {
        CoreEvent::Thinking { summary } => UiEvent::Thinking { summary },
        CoreEvent::Editing {
            target,
            action,
            reason,
        } => UiEvent::Editing {
            target,
            action: match reason {
                Some(reason) if !reason.is_empty() => format!("{action} ({reason})"),
                _ => action,
            },
        },
        CoreEvent::Plan { steps } => UiEvent::Plan { steps },
        CoreEvent::Execution { step } => UiEvent::Execution { step },
        CoreEvent::Preview { diff } => UiEvent::Preview { diff },
        CoreEvent::Result { message } => UiEvent::Result { message },
        CoreEvent::Pipeline { state } => UiEvent::Pipeline { state },
        CoreEvent::Next { actions } => UiEvent::Next { actions },
        CoreEvent::Error { message } => UiEvent::Error { message },
        CoreEvent::Debug { message } => UiEvent::Debug { message },
        CoreEvent::Proposal { candidates } => UiEvent::Proposal { candidates },
    }
}

pub fn handle_submit(
    state: &mut TuiState,
    core: &dyn CoreExecutor,
    input: String,
    working_dir: PathBuf,
) {
    eprintln!("[UI] Input received");
    let is_select = input.trim().to_ascii_lowercase().starts_with("select ");
    let request = CoreRequest::new(
        input,
        working_dir,
        state.pipeline_state.clone(),
        Some(state.design_doc.clone()),
        state.current_proposals.clone(),
    );
    let response = core.execute(request);
    let success = response.status != crate::core::ExecutionStatus::Failed;

    apply_core_response(
        &mut state.event_queue,
        &mut state.pipeline_state,
        response.events,
    );

    if success && is_select {
        state.current_proposals = None;
    }
    if success && let Some(design) = response.design {
        state.update_design(design);
    }
}

fn apply_core_response(
    queue: &mut EventQueue,
    pipeline_state: &mut PipelineState,
    events: Vec<CoreEvent>,
) {
    for event in events {
        if let CoreEvent::Pipeline { state } = &event
            && let Some(next) = pipeline_state_from_label(state)
        {
            *pipeline_state = next;
        }
        let is_error = matches!(event, CoreEvent::Error { .. });
        eprintln!("[UI] Rendering event");
        queue.push(to_ui_event(event));
        if is_error {
            break;
        }
    }
}

fn pipeline_state_from_label(label: &str) -> Option<PipelineState> {
    match label {
        "Idle" => Some(PipelineState::Idle),
        "Proposed" => Some(PipelineState::Proposed),
        "Planned" => Some(PipelineState::Planned),
        "Previewed" => Some(PipelineState::Previewed),
        "Applied" => Some(PipelineState::Applied),
        "Staged" => Some(PipelineState::Staged),
        "Committed" => Some(PipelineState::Committed),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{CoreResponse, ExecutionStatus};
    use crate::tui::model::{TraceStatsViewModel, TraceViewModel, UiPayload};

    #[derive(Default)]
    struct FakeCore {
        response: Option<CoreResponse>,
    }

    impl CoreExecutor for FakeCore {
        fn execute(&self, _request: CoreRequest) -> CoreResponse {
            self.response.clone().unwrap_or(CoreResponse {
                events: vec![CoreEvent::Result {
                    message: "done".to_string(),
                }],
                status: ExecutionStatus::Executed,
                design: None,
            })
        }
    }

    #[test]
    fn submit_forwards_input_and_renders_events() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![
                    CoreEvent::Result {
                        message: "ok".to_string(),
                    },
                    CoreEvent::Pipeline {
                        state: "Proposed".to_string(),
                    },
                ],
                status: ExecutionStatus::Proposed,
                design: None,
            }),
        };

        handle_submit(&mut state, &core, "fix parser bug".to_string(), ".".into());
        state.handle_ui_events();

        assert_eq!(state.pipeline_state, PipelineState::Proposed);
        assert!(
            state
                .flattened_chat_lines()
                .iter()
                .any(|line| line == "[RESULT] ok")
        );
    }

    fn empty_payload() -> UiPayload {
        UiPayload {
            trace: TraceViewModel {
                request_id: "test".to_string(),
                steps: vec![],
                stats: TraceStatsViewModel {
                    total_nodes: 0,
                    max_depth: 0,
                    recall_hit_rate: 0.0,
                    avg_branching: 0.0,
                },
            },
            hypotheses: vec![],
            memory: vec![],
            selected: None,
        }
    }
}
