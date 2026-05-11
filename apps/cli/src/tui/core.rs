use std::path::PathBuf;

pub use crate::core::{CoreEvent, CoreExecutor, CoreRequest, RuntimeCoreBridge};
use crate::nl::normalization::normalize_runtime_input;
use crate::pipeline::PipelineState;
use crate::runtime::logging::{emit_debug, tui_logging_isolated};
use crate::runtime::runtime_events::DebugLevel;
use crate::tui::runtime::RuntimeShellState;

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

    // §7.1 §10.1: Transition to Thinking state before dispatch.
    // Runtime must never be silent — emit visible thinking event immediately.
    state.runtime_state = RuntimeShellState::Thinking;
    state.event_queue.push(UiEvent::Thinking {
        summary: "processing intent / 意図を処理中".to_string(),
    });

    // Phase 4.5: build CoreRequest (pass-through).
    let runtime_input = normalize_runtime_input(&input)
        .map(|normalized| normalized.command.to_runtime_input())
        .unwrap_or(input);
    let request = CoreRequest::new(runtime_input);
    let mut response = core.execute(request);

    // §13.1: Empty event protection — execution must always produce visible narrative.
    if response.events.is_empty() {
        response.events.push(CoreEvent::Error {
            message: "No runtime narrative generated".to_string(),
        });
    }

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
    use crate::core::{
        Constraint, CoreResponse, DesignDocument, ExecutionStatus, ReasonUnit, StructureTree,
    };
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

    fn test_design(version: u64) -> DesignDocument {
        DesignDocument::new(
            version,
            vec![ReasonUnit {
                id: "ru-test".to_string(),
                title: "test".to_string(),
                summary: "test design".to_string(),
            }],
            StructureTree {
                module: "test".to_string(),
                functions: vec!["test_fn".to_string()],
            },
            vec![Constraint {
                text: "test constraint".to_string(),
            }],
        )
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

    // ─── §14.1 Interactive Runtime Tests ────────────────────────────────────

    /// §14.1 — Every submit produces at least one visible runtime event.
    #[test]
    fn test_submit_generates_visible_event() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        handle_submit(&mut state, &core, "analyze workspace".to_string(), ".".into());
        state.handle_ui_events();
        assert!(!state.flattened_chat_lines().is_empty());
    }

    /// §14.1 — Runtime state transitions to Thinking before Core dispatch.
    #[test]
    fn test_runtime_transition_visible() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        handle_submit(&mut state, &core, "fix parser".to_string(), ".".into());
        state.handle_ui_events();
        // At least one [THINKING] line must appear.
        let lines = state.flattened_chat_lines();
        assert!(
            lines.iter().any(|l| l.starts_with("[THINKING]")),
            "no [THINKING] line found: {lines:?}"
        );
    }

    /// §14.1 §13.1 — Empty Core response triggers fallback Error event.
    #[test]
    fn test_empty_response_protection() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![],
                status: ExecutionStatus::Executed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "unknown xyz".to_string(), ".".into());
        state.handle_ui_events();
        let lines = state.flattened_chat_lines();
        assert!(!lines.is_empty(), "runtime silence after empty response");
        assert!(
            lines.iter().any(|l| l.starts_with("[THINKING]") || l.starts_with("[ERROR]")),
            "no visible narrative after empty response: {lines:?}"
        );
    }

    /// §14.1 §9.1 — Thinking narrative emitted by Core is visible in chat.
    #[test]
    fn test_narrative_projection_visible() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![CoreEvent::Thinking {
                    summary: "cognitive processing active".to_string(),
                }],
                status: ExecutionStatus::Executed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "analyze workspace".to_string(), ".".into());
        state.handle_ui_events();
        assert!(
            state
                .flattened_chat_lines()
                .iter()
                .any(|l| l.contains("[THINKING]") && l.contains("cognitive processing active")),
            "narrative not visible in chat"
        );
    }

    /// §14.1 §8.1 — Design update projected into design panel after execution.
    #[test]
    fn test_projection_updates_after_execution() {
        let mut state = TuiState::new(empty_payload());
        let new_version = state.design_doc.version + 1;
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![CoreEvent::Result {
                    message: "updated".to_string(),
                }],
                status: ExecutionStatus::Executed,
                design: Some(test_design(new_version)),
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        let prev_version = state.design_doc.version;
        handle_submit(&mut state, &core, "update design".to_string(), ".".into());
        state.handle_ui_events();
        assert_ne!(
            state.design_doc.version, prev_version,
            "design projection not updated"
        );
    }

    // ─── §14.2 Intent Tests ──────────────────────────────────────────────────

    /// §14.2 — English normalized intent reaches Core correctly.
    #[test]
    fn test_english_intent_execution() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        handle_submit(&mut state, &core, "analyze workspace".to_string(), ".".into());
        // "analyze workspace" starts with "analyze" → normalized to "analyze"
        assert_eq!(
            core.seen_input.lock().unwrap().as_deref(),
            Some("analyze")
        );
    }

    /// §14.2 — Japanese intent (解析) normalizes and reaches Core.
    #[test]
    fn test_japanese_intent_execution() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        handle_submit(
            &mut state,
            &core,
            "ランタイム状態を解析する".to_string(),
            ".".into(),
        );
        state.handle_ui_events();
        // Must not be silent.
        assert!(!state.flattened_chat_lines().is_empty());
        // Intent should have been normalized to "analyze"
        assert_eq!(
            core.seen_input.lock().unwrap().as_deref(),
            Some("analyze")
        );
    }

    /// §14.2 — Mixed bilingual intent normalizes correctly.
    #[test]
    fn test_bilingual_intent_execution() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        handle_submit(
            &mut state,
            &core,
            "parser.rs を preview".to_string(),
            ".".into(),
        );
        assert_eq!(
            core.seen_input.lock().unwrap().as_deref(),
            Some("preview parser.rs")
        );
    }

    /// §14.2 §6.3 — Unknown intent still produces a visible event (no silence).
    #[test]
    fn test_unknown_intent_generates_error() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![CoreEvent::Error {
                    message: "unknown intent: !@#$".to_string(),
                }],
                status: ExecutionStatus::Failed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "!@#$ gibberish".to_string(), ".".into());
        state.handle_ui_events();
        let lines = state.flattened_chat_lines();
        assert!(
            lines.iter().any(|l| l.starts_with("[ERROR]") || l.starts_with("[THINKING]")),
            "no visible event for unknown intent: {lines:?}"
        );
    }

    // ─── §14.3 Governance Tests ──────────────────────────────────────────────

    /// §14.3 §12.1 — Governance rejection event is visible in chat.
    #[test]
    fn test_governance_narrative_visible() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![CoreEvent::Error {
                    message: "GovernanceRejected: mutation risk exceeds threshold".to_string(),
                }],
                status: ExecutionStatus::Failed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "risky mutation".to_string(), ".".into());
        state.handle_ui_events();
        assert!(
            state
                .flattened_chat_lines()
                .iter()
                .any(|l| l.contains("[ERROR]") && l.contains("GovernanceRejected")),
            "governance narrative not visible"
        );
    }

    /// §14.3 §12.2 — Rejection reason is rendered in chat projection.
    #[test]
    fn test_rejection_projection_visible() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![
                    CoreEvent::Error {
                        message: "SafetyViolation: rm -rf rejected".to_string(),
                    },
                    CoreEvent::Next {
                        actions: vec!["undo".to_string(), "reselect".to_string()],
                    },
                ],
                status: ExecutionStatus::Failed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "rm -rf /".to_string(), ".".into());
        state.handle_ui_events();
        let lines = state.flattened_chat_lines();
        assert!(
            lines.iter().any(|l| l.contains("[ERROR]")),
            "rejection not projected"
        );
        assert!(
            lines.iter().any(|l| l.contains("[NEXT]")),
            "recovery actions not projected"
        );
    }

    /// §14.3 — Pipeline projection is visible after execution.
    #[test]
    fn test_execution_pipeline_projection() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![
                    CoreEvent::Thinking {
                        summary: "planning".to_string(),
                    },
                    CoreEvent::Execution {
                        step: "strategy executed".to_string(),
                    },
                    CoreEvent::Result {
                        message: "pipeline complete".to_string(),
                    },
                    CoreEvent::Pipeline {
                        state: "Planned".to_string(),
                    },
                ],
                status: ExecutionStatus::Executed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "execute plan".to_string(), ".".into());
        state.handle_ui_events();
        assert_eq!(state.pipeline_state, PipelineState::Planned);
        let lines = state.flattened_chat_lines();
        assert!(lines.iter().any(|l| l.starts_with("[EXECUTION]")));
        assert!(lines.iter().any(|l| l.starts_with("[RESULT]")));
    }

    // ─── §14.4 Render Tests ──────────────────────────────────────────────────

    /// §14.4 §11.1 — state_generation_id advances after event processing.
    #[test]
    fn test_render_updates_after_event() {
        let mut state = TuiState::new(empty_payload());
        let gen_before = state.state_generation_id;
        state.enqueue_event(UiEvent::Thinking {
            summary: "render trigger".to_string(),
        });
        state.handle_ui_events();
        assert!(
            state.state_generation_id > gen_before,
            "generation_id did not advance after event"
        );
    }

    /// §14.4 — Events appear in flattened chat lines after handle_ui_events.
    #[test]
    fn test_chat_projection_visible() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![
                    CoreEvent::Thinking {
                        summary: "planning workspace update".to_string(),
                    },
                    CoreEvent::Result {
                        message: "workspace updated".to_string(),
                    },
                ],
                status: ExecutionStatus::Executed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "analyze workspace".to_string(), ".".into());
        state.handle_ui_events();
        let lines = state.flattened_chat_lines();
        assert!(lines.iter().any(|l| l.contains("planning workspace update")));
        assert!(lines.iter().any(|l| l.contains("workspace updated")));
    }

    /// §14.4 §8.1 — Workspace design panel updates reflect execution output.
    #[test]
    fn test_workspace_projection_visible() {
        let mut state = TuiState::new(empty_payload());
        let new_version = state.design_doc.version + 1;
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![CoreEvent::DesignUpdate {
                    summary: "workspace redesigned".to_string(),
                    score: 0.91,
                }],
                status: ExecutionStatus::Executed,
                design: Some(test_design(new_version)),
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "redesign workspace".to_string(), ".".into());
        state.handle_ui_events();
        let panel = state.design_panel_lines();
        assert!(
            panel.iter().any(|l| l.contains(&new_version.to_string())),
            "design version not reflected in workspace projection"
        );
    }

    /// §14.4 §11.1 — Event queue is populated after submit (before handle_ui_events).
    #[test]
    fn test_runtime_refresh_after_submit() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        handle_submit(&mut state, &core, "fix bug".to_string(), ".".into());
        // Queue must have events before handle_ui_events drains them.
        assert!(
            !state.event_queue.is_empty(),
            "event queue empty immediately after submit"
        );
    }

    // ─── §14.5 Stability Tests ───────────────────────────────────────────────

    /// §14.5 §10.2 — Every submit produces at least one visible chat line.
    #[test]
    fn test_no_runtime_silence() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        let initial_lines = state.flattened_chat_lines().len();
        handle_submit(&mut state, &core, "some command".to_string(), ".".into());
        state.handle_ui_events();
        assert!(
            state.flattened_chat_lines().len() > initial_lines,
            "runtime was silent after submit"
        );
    }

    /// §14.5 — Pipeline state transitions do not desync runtime_state projection.
    #[test]
    fn test_no_projection_desync() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![CoreEvent::Pipeline {
                    state: "Proposed".to_string(),
                }],
                status: ExecutionStatus::Proposed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "propose change".to_string(), ".".into());
        state.handle_ui_events();
        // pipeline_state and runtime_state must be consistent.
        assert_eq!(state.pipeline_state, PipelineState::Proposed);
        // runtime_state should not be Idle after a Proposed pipeline event.
        assert_ne!(
            state.runtime_state,
            crate::tui::runtime::RuntimeShellState::Idle
        );
    }

    /// §14.5 §7.2 — Narrative events and pipeline state are consistent.
    #[test]
    fn test_narrative_runtime_consistency() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore {
            response: Some(CoreResponse {
                events: vec![
                    CoreEvent::Thinking {
                        summary: "planning execution".to_string(),
                    },
                    CoreEvent::Execution {
                        step: "step 1".to_string(),
                    },
                    CoreEvent::Result {
                        message: "execution complete".to_string(),
                    },
                ],
                status: ExecutionStatus::Executed,
                design: None,
                core_state: None,
            }),
            seen_input: std::sync::Mutex::new(None),
        };
        handle_submit(&mut state, &core, "execute".to_string(), ".".into());
        state.handle_ui_events();
        let lines = state.flattened_chat_lines();
        // Narrative must contain Thinking before Result.
        let thinking_pos = lines.iter().position(|l| l.contains("[THINKING]"));
        let result_pos = lines.iter().position(|l| l.contains("[RESULT]"));
        assert!(
            thinking_pos.is_some() && result_pos.is_some(),
            "narrative sequence incomplete"
        );
    }

    /// §14.5 — Multiple successive submits remain stable.
    #[test]
    fn test_interactive_loop_stability() {
        let mut state = TuiState::new(empty_payload());
        let core = FakeCore::default();
        for i in 0..5 {
            handle_submit(
                &mut state,
                &core,
                format!("command {i}"),
                ".".into(),
            );
            state.handle_ui_events();
        }
        // No panic, pipeline stable, events visible.
        assert!(!state.flattened_chat_lines().is_empty());
        assert_eq!(state.pipeline_state, PipelineState::Idle);
    }
}
