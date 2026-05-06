use std::path::PathBuf;

pub use crate::core::{CoreEvent, CoreExecutor, CoreRequest, RuntimeCoreBridge};
use crate::nl::normalization::normalize_runtime_input;
use crate::pipeline::PipelineState;
use crate::runtime::logging::{emit_debug, tui_logging_isolated};
use crate::runtime::runtime_events::DebugLevel;

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
        CoreEvent::Diff { file, changes } => UiEvent::Diff { file, changes },
        CoreEvent::Result { message } => UiEvent::Result { message },
        CoreEvent::DesignUpdate { summary, score } => UiEvent::DesignUpdate { summary, score },
        CoreEvent::DesignDiff { changes } => UiEvent::DesignDiff { changes },
        CoreEvent::Pipeline { state } => UiEvent::Pipeline { state },
        CoreEvent::Next { actions } => UiEvent::Next { actions },
        CoreEvent::Error { message } => UiEvent::Error { message },
        CoreEvent::ErrorRecovery { candidates } => UiEvent::ErrorRecovery { candidates },
        CoreEvent::Debug { message } => UiEvent::Debug { message },
        CoreEvent::Proposal { candidates } => UiEvent::Proposal { candidates },
    }
}

pub fn handle_submit(
    state: &mut TuiState,
    core: &dyn CoreExecutor,
    input: String,
    _working_dir: PathBuf,
) {
    let _event = emit_debug("UI", "Input received", DebugLevel::Debug);
    // Phase 4.5: build CoreRequest (pass-through).
    let runtime_input = normalize_runtime_input(&input)
        .map(|normalized| normalized.command.to_runtime_input())
        .unwrap_or(input);
    let request = CoreRequest::new(runtime_input);
    let response = core.execute(request);
    let success = response.status != crate::core::ExecutionStatus::Failed;

    // Phase 4.5: sync core_snapshot first so downstream render reads correct state.
    if let Some(snapshot) = response.core_state {
        state.core_snapshot = snapshot.clone();
        state.pipeline_state = snapshot.status.clone();
    }

    apply_core_response(
        &mut state.event_queue,
        &mut state.pipeline_state,
        response.events,
    );

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
        if !tui_logging_isolated() {
            let _event = emit_debug("UI", "Rendering event", DebugLevel::Trace);
        }
        queue.push(to_ui_event(event));
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
        seen_input: std::sync::Mutex<Option<String>>,
    }

    impl CoreExecutor for FakeCore {
        fn execute(&self, request: CoreRequest) -> CoreResponse {
            *self.seen_input.lock().expect("seen input") = Some(request.raw.clone());
            self.response.clone().unwrap_or(CoreResponse {
                events: vec![CoreEvent::Result {
                    message: "done".to_string(),
                }],
                status: ExecutionStatus::Executed,
                design: None,
                core_state: None,
            })
        }
    }

    #[test]
    fn submit_normalizes_japanese_runtime_intent_before_core() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();

        handle_submit(
            &mut state,
            &core,
            "parser.rs を preview".to_string(),
            ".".into(),
        );

        assert_eq!(
            core.seen_input.lock().expect("seen").as_deref(),
            Some("preview parser.rs")
        );
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
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
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
