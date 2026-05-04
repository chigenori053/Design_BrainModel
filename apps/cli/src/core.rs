use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use design_search_engine::stable_v03::DeterministicBeamSearchEngine;
use memory_space_phase14::stable_v03::InMemoryEngine;
use runtime_core::{CoreRuntime, RuntimeExecutionResult};
use serde_json::json;
use sha2::{Digest, Sha256};
use strategy_engine::{
    DryRunIntegrator, ExecutionContext as StrategyExecutionContext, ExecutionHistory,
    ExecutionPlanCandidate, Intent, StrategyEngine, StrategyInput, StrategyOutput,
    generate_candidates_from_intent, requires_clarification,
};

use crate::pipeline::PipelineState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreRequest {
    pub input: String,
    pub context: ExecutionContext,
}

const DESIGN_MAX_LINES: usize = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasonUnit {
    pub id: String,
    pub title: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructureTree {
    pub module: String,
    pub functions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Constraint {
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignDocument {
    pub version: u64,
    pub reason_units: Vec<ReasonUnit>,
    pub structure: StructureTree,
    pub constraints: Vec<Constraint>,
    pub rendered: Vec<String>,
}

impl DesignDocument {
    pub fn new(
        version: u64,
        reason_units: Vec<ReasonUnit>,
        structure: StructureTree,
        constraints: Vec<Constraint>,
    ) -> Self {
        let mut doc = Self {
            version,
            reason_units,
            structure,
            constraints,
            rendered: Vec::new(),
        };
        doc.regenerate_rendered();
        doc
    }

    pub fn regenerate_rendered(&mut self) {
        let mut rendered = vec!["[DESIGN]".to_string(), String::new()];
        rendered.push(format!("Module: {}", self.structure.module));
        for function in &self.structure.functions {
            rendered.push(format!("- {function}"));
        }

        if !self.reason_units.is_empty() {
            rendered.push(String::new());
            rendered.push("Reason Units:".to_string());
            for unit in &self.reason_units {
                rendered.push(format!("- {}: {}", unit.title, unit.summary));
            }
        }

        if !self.constraints.is_empty() {
            rendered.push(String::new());
            rendered.push("Constraints:".to_string());
            for constraint in &self.constraints {
                rendered.push(format!("- {}", constraint.text));
            }
        }

        rendered.truncate(DESIGN_MAX_LINES);
        self.rendered = rendered;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionContext {
    pub working_dir: PathBuf,
    pub pipeline_state: PipelineState,
    pub design_snapshot: Option<DesignDocument>,
    /// Proposal candidates awaiting user selection.  Phase 1C.5 §5.3.
    pub current_proposals: Option<Vec<ExecutionPlanCandidate>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreResponse {
    pub events: Vec<CoreEvent>,
    pub status: ExecutionStatus,
    pub design: Option<DesignDocument>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreEvent {
    Thinking {
        summary: String,
    },
    Editing {
        target: String,
        action: String,
        reason: Option<String>,
    },
    Plan {
        steps: Vec<String>,
    },
    Execution {
        step: String,
    },
    Preview {
        diff: Vec<String>,
    },
    Result {
        message: String,
    },
    Pipeline {
        state: String,
    },
    Next {
        actions: Vec<String>,
    },
    Error {
        message: String,
    },
    Debug {
        message: String,
    },
    /// Structured execution proposal.  Spec DBM-EXECUTION-CANDIDATE-SPEC §8.
    Proposal {
        candidates: Vec<ExecutionPlanCandidate>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionStatus {
    Idle,
    Proposed,
    Planned,
    Executed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TraceLevel {
    Off,
    Error,
    Basic,
    Full,
}

const TRACE_LEVEL: TraceLevel = TraceLevel::Full;

macro_rules! trace_ir {
    ($level:expr, $stage:expr, $data:expr) => {{
        if trace_enabled($level) {
            emit_core_log($stage, $data.to_string());
        }
    }};
}

fn emit_core_log(stage: &str, data: String) {
    let line = format!("[IR-TRACE][{stage}] {data}\n");
    let _ = std::io::Write::write_all(&mut std::io::stderr(), line.as_bytes());
}

pub trait CoreExecutor {
    fn execute(&self, request: CoreRequest) -> CoreResponse;
}

impl CoreRequest {
    pub fn new(
        input: String,
        working_dir: PathBuf,
        pipeline_state: PipelineState,
        design_snapshot: Option<DesignDocument>,
        current_proposals: Option<Vec<ExecutionPlanCandidate>>,
    ) -> Self {
        Self {
            input,
            context: ExecutionContext {
                working_dir,
                pipeline_state,
                design_snapshot,
                current_proposals,
            },
        }
    }
}

pub struct RuntimeCoreBridge {
    runtime: CoreRuntime,
    strategy: StrategyEngine,
    pending_files: Mutex<Vec<PendingFile>>,
    applied_files: Mutex<Vec<AppliedFile>>,
}

impl RuntimeCoreBridge {
    pub fn new(runtime: CoreRuntime, strategy: StrategyEngine) -> Self {
        Self {
            runtime,
            strategy,
            pending_files: Mutex::new(Vec::new()),
            applied_files: Mutex::new(Vec::new()),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(
            CoreRuntime::new_with_defaults(
                Arc::new(InMemoryEngine::default()),
                Arc::new(DeterministicBeamSearchEngine::default()),
            ),
            StrategyEngine::default(),
        )
    }
}

impl Default for RuntimeCoreBridge {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl CoreExecutor for RuntimeCoreBridge {
    fn execute(&self, request: CoreRequest) -> CoreResponse {
        if let Some(response) = self.execute_pipeline_command(&request) {
            return response;
        }

        let mut events = vec![
            CoreEvent::Thinking {
                summary: "refining natural language intent".to_string(),
            },
            trace_event(
                "INTENT",
                json!({
                    "raw_input": request.input,
                    "pipeline_state": request.context.pipeline_state.label(),
                    "timestamp": timestamp_millis(),
                }),
            ),
            CoreEvent::Pipeline {
                state: request.context.pipeline_state.label().to_string(),
            },
        ];
        let intent = Intent::new(request.input.clone());
        let ambiguous = requires_clarification(&intent);
        trace_ir!(
            TraceLevel::Basic,
            "CLARIFICATION",
            format!(
                "action={:?}, file={:?}, symbol={:?}, ambiguous={}",
                intent.action, intent.file, intent.symbol, ambiguous
            )
        );
        if ambiguous {
            let candidates = generate_candidates_from_intent(&intent);
            trace_ir!(TraceLevel::Basic, "PROPOSAL_GENERATED", candidates.len());
            events.push(CoreEvent::Proposal { candidates });
            events.push(CoreEvent::Pipeline {
                state: PipelineState::Proposed.label().to_string(),
            });
            events.push(CoreEvent::Next {
                actions: vec!["select <n> で候補を選択".to_string()],
            });
            return CoreResponse {
                events,
                status: ExecutionStatus::Proposed,
                design: None,
            };
        }

        let chat_context = runtime_core::ChatContext::default();
        let runtime_result = match self
            .runtime
            .execute_from_text(&request.input, &chat_context)
        {
            Ok(RuntimeExecutionResult::Executed(result)) => result,
            Ok(RuntimeExecutionResult::Clarification(clarification)) => {
                if append_clear_intent_runtime_clarification_events(
                    &mut events,
                    &intent,
                    &clarification,
                ) {
                    return CoreResponse {
                        events,
                        status: ExecutionStatus::Executed,
                        design: None,
                    };
                }

                let trace = ir_trace_json(
                    "ERROR",
                    json!({
                        "status": "clarification_required",
                        "error": clarification.message,
                        "timestamp": timestamp_millis(),
                    }),
                );
                trace_ir!(TraceLevel::Error, "ERROR", trace);
                events.push(CoreEvent::Error {
                    message: format!("clarification required: {}", clarification.message),
                });
                return CoreResponse {
                    events,
                    status: ExecutionStatus::Failed,
                    design: None,
                };
            }
            Err(err) => {
                let trace = ir_trace_json(
                    "ERROR",
                    json!({
                        "status": "runtime_error",
                        "error": format!("{err:?}"),
                        "timestamp": timestamp_millis(),
                    }),
                );
                trace_ir!(TraceLevel::Error, "ERROR", trace);
                events.push(CoreEvent::Error {
                    message: format!("core execution failed: {err:?}"),
                });
                return CoreResponse {
                    events,
                    status: ExecutionStatus::Failed,
                    design: None,
                };
            }
        };

        events.push(CoreEvent::Thinking {
            summary: "strategy execution started".to_string(),
        });
        events.push(trace_event(
            "IR",
            ir_plan_json(&runtime_result.execution_plan),
        ));
        events.push(trace_event(
            "EXEC_PLAN",
            execution_plan_json(&runtime_result.execution_plan),
        ));

        {
            let ir = &runtime_result.execution_plan;
            let ir_steps = ir.dependency_plan.install_commands.len()
                + ir.build_plan.build_commands.len()
                + ir.run_plan.run_commands.len()
                + ir.test_plan.test_commands.len();
            trace_ir!(TraceLevel::Basic, "COUNT", format!("IR_STEPS={ir_steps}"));
        }

        let strategy_input = StrategyInput {
            intent,
            initial_plan: runtime_result.execution_plan.clone(),
            context: StrategyExecutionContext {
                repo_root: request.context.working_dir.clone(),
                ..StrategyExecutionContext::default()
            },
            history: ExecutionHistory::new(),
        };
        let runner = DryRunIntegrator;
        let strategy_output = self.strategy.execute(strategy_input, &runner);
        events.push(trace_event(
            "CANDIDATES",
            candidates_json_from_strategy(&strategy_output),
        ));
        events.push(trace_event(
            "SELECTED",
            selected_json_from_strategy(&strategy_output),
        ));
        events.push(trace_event(
            "EXECUTION",
            execution_result_json(&strategy_output),
        ));
        events.push(CoreEvent::Execution {
            step: "strategy execution completed".to_string(),
        });
        events.extend(core_events_from_strategy(&strategy_output));

        if !strategy_output.success {
            let step = selected_json_from_strategy(&strategy_output);
            let trace = ir_trace_json(
                "ERROR",
                json!({
                    "error": strategy_output.strategy_trace.final_outcome.to_string(),
                    "selected": step,
                    "timestamp": timestamp_millis(),
                }),
            );
            trace_ir!(TraceLevel::Error, "ERROR", trace);
            events.push(CoreEvent::Error {
                message: strategy_output.strategy_trace.final_outcome.to_string(),
            });
            return CoreResponse {
                events,
                status: ExecutionStatus::Failed,
                design: None,
            };
        }

        self.store_pending_files(&runtime_result);

        events.push(CoreEvent::Result {
            message: "core execution completed".to_string(),
        });
        events.push(CoreEvent::Pipeline {
            state: PipelineState::Planned.label().to_string(),
        });
        let pending = self.pending_files.lock().expect("pending lock").clone();
        events.push(CoreEvent::Preview {
            diff: if pending.is_empty() {
                vec!["(no pending files)".to_string()]
            } else {
                preview_lines(&pending)
            },
        });
        events.push(CoreEvent::Pipeline {
            state: PipelineState::Previewed.label().to_string(),
        });
        events.push(CoreEvent::Next {
            actions: vec!["apply".to_string()],
        });

        let design = design_document_from_core_result(
            &runtime_result,
            &strategy_output,
            request.context.design_snapshot.as_ref(),
        );
        CoreResponse {
            events,
            status: ExecutionStatus::Executed,
            design: Some(design),
        }
    }
}

impl RuntimeCoreBridge {
    fn execute_pipeline_command(&self, request: &CoreRequest) -> Option<CoreResponse> {
        let input = request.input.trim();
        let lower = input.to_ascii_lowercase();
        match lower.as_str() {
            "preview" => Some(self.preview(request)),
            "apply" => Some(self.apply(request)),
            "git commit" | "commit" | "commit changes" => Some(self.git_commit(request)),
            "rollback" => Some(self.rollback(request)),
            _ if lower.starts_with("git add ") => Some(self.git_add(request, input)),
            _ if lower.starts_with("select ") => Some(self.select_candidate(request, input)),
            _ if is_forbidden_command(&lower) => Some(error_response("SafetyViolation", input)),
            _ => None,
        }
    }

    fn preview(&self, request: &CoreRequest) -> CoreResponse {
        if request.context.pipeline_state != PipelineState::Planned {
            trace_unsupported_operation("preview", "Preview", None, "requires Planned state");
            return error_response("ExecutionError", "preview requires Planned state");
        }
        let pending = self.pending_files.lock().expect("pending lock").clone();
        if pending.is_empty() {
            return error_response("ValidationError", "no pending generated files to preview");
        }

        CoreResponse {
            events: vec![
                CoreEvent::Preview {
                    diff: preview_lines(&pending),
                },
                CoreEvent::Pipeline {
                    state: PipelineState::Previewed.label().to_string(),
                },
                CoreEvent::Next {
                    actions: vec!["apply".to_string()],
                },
            ],
            status: ExecutionStatus::Planned,
            design: None,
        }
    }

    fn apply(&self, request: &CoreRequest) -> CoreResponse {
        if request.context.pipeline_state != PipelineState::Previewed {
            trace_unsupported_operation("apply", "Apply", None, "requires Previewed state");
            return error_response("ExecutionError", "apply requires Previewed state");
        }
        let pending = self.pending_files.lock().expect("pending lock").clone();
        if pending.is_empty() {
            return error_response("ValidationError", "no pending generated files to apply");
        }

        let mut applied = Vec::new();
        for file in &pending {
            let target = match resolve_repo_file(&request.context.working_dir, &file.path) {
                Ok(target) => target,
                Err(err) => return error_response("SafetyViolation", &err),
            };
            let before = fs::read(&target).ok();
            let before_checksum = before.as_ref().map(|content| checksum_bytes(content));
            if let Some(parent) = target.parent()
                && let Err(err) = fs::create_dir_all(parent)
            {
                return error_response(
                    "ExecutionError",
                    &format!("create directory failed: {err}"),
                );
            }
            if let Err(err) = fs::write(&target, file.content.as_bytes()) {
                return error_response("ExecutionError", &format!("apply failed: {err}"));
            }
            let after = match fs::read(&target) {
                Ok(content) => content,
                Err(err) => {
                    restore_applied(&applied);
                    return error_response("ExecutionError", &format!("verify failed: {err}"));
                }
            };
            if checksum_bytes(&after) != file.content_checksum {
                restore_applied(&applied);
                return error_response("ChecksumMismatch", &file.path);
            }
            applied.push(AppliedFile {
                path: file.path.clone(),
                target,
                backup: before,
                before_checksum,
                after_checksum: checksum_bytes(&after),
            });
        }

        *self.applied_files.lock().expect("applied lock") = applied.clone();
        let _snapshot = sync_pipeline_with_git(&request.context.working_dir)
            .unwrap_or_else(|_| GitSnapshot::from_applied(&applied));
        CoreResponse {
            events: vec![
                CoreEvent::Result {
                    message: "Changes applied".to_string(),
                },
                CoreEvent::Pipeline {
                    state: PipelineState::Applied.label().to_string(),
                },
                CoreEvent::Next {
                    actions: vec!["git add <file>".to_string(), "commit changes".to_string()],
                },
            ],
            status: ExecutionStatus::Executed,
            design: None,
        }
    }

    fn git_add(&self, request: &CoreRequest, input: &str) -> CoreResponse {
        if request.context.pipeline_state != PipelineState::Applied {
            trace_unsupported_operation(input, "GitAdd", None, "requires Applied state");
            return error_response("ExecutionError", "git add requires Applied state");
        }
        let Some(path) = input.strip_prefix("git add ").map(str::trim) else {
            return error_response("ValidationError", "git add requires one explicit file");
        };
        if let Err(err) = validate_git_add_path(path) {
            trace_unsupported_operation(input, "GitAdd", Some(path), &err);
            return error_response("SafetyViolation", &err);
        }
        match run_git(&request.context.working_dir, &["add", "--", path]) {
            Ok(_) => {}
            Err(err) => return error_response("ExecutionError", &err),
        }
        let snapshot = match sync_pipeline_with_git(&request.context.working_dir) {
            Ok(snapshot) => snapshot,
            Err(err) => return error_response("ExecutionError", &err),
        };
        if snapshot.staged.is_empty() {
            return error_response("ExecutionError", "git add produced no staged changes");
        }
        CoreResponse {
            events: vec![
                CoreEvent::Pipeline {
                    state: PipelineState::Staged.label().to_string(),
                },
                CoreEvent::Result {
                    message: format!("Staged: {path}"),
                },
                CoreEvent::Next {
                    actions: vec!["commit changes".to_string()],
                },
            ],
            status: ExecutionStatus::Executed,
            design: None,
        }
    }

    fn git_commit(&self, request: &CoreRequest) -> CoreResponse {
        if request.context.pipeline_state != PipelineState::Staged {
            trace_unsupported_operation("git commit", "GitCommit", None, "requires Staged state");
            return error_response("ExecutionError", "commit requires Staged state");
        }
        if let Err(err) = run_git(
            &request.context.working_dir,
            &[
                "-c",
                "user.name=DEM CLI",
                "-c",
                "user.email=dem-cli@example.invalid",
                "commit",
                "-m",
                "auto-generated",
            ],
        ) {
            return error_response("ExecutionError", &err);
        }
        let hash = run_git(
            &request.context.working_dir,
            &["rev-parse", "--short", "HEAD"],
        )
        .map(|out| out.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
        let _snapshot = sync_pipeline_with_git(&request.context.working_dir).unwrap_or_default();
        CoreResponse {
            events: vec![
                CoreEvent::Result {
                    message: format!("Committed: {hash}"),
                },
                CoreEvent::Pipeline {
                    state: PipelineState::Committed.label().to_string(),
                },
                CoreEvent::Next {
                    actions: vec!["continue development".to_string()],
                },
            ],
            status: ExecutionStatus::Executed,
            design: None,
        }
    }

    fn rollback(&self, request: &CoreRequest) -> CoreResponse {
        if request.context.pipeline_state == PipelineState::Committed {
            trace_unsupported_operation("rollback", "Rollback", None, "committed state");
            return error_response("ExecutionError", "RollbackForbidden");
        }
        let applied = self.applied_files.lock().expect("applied lock").clone();
        if applied.is_empty() {
            return error_response("ExecutionError", "no applied changes to rollback");
        }
        restore_applied(&applied);
        self.applied_files.lock().expect("applied lock").clear();
        CoreResponse {
            events: vec![
                CoreEvent::Result {
                    message: "Rollback completed".to_string(),
                },
                CoreEvent::Pipeline {
                    state: PipelineState::Previewed.label().to_string(),
                },
                CoreEvent::Next {
                    actions: vec!["apply".to_string()],
                },
            ],
            status: ExecutionStatus::Planned,
            design: None,
        }
    }

    fn store_pending_files(&self, result: &runtime_core::stable_v03::RuntimeResult) {
        let files = result
            .project_layout
            .files
            .iter()
            .map(|file| PendingFile {
                path: file.path.clone(),
                content: file.content.clone(),
                content_checksum: checksum_bytes(file.content.as_bytes()),
            })
            .collect::<Vec<_>>();
        *self.pending_files.lock().expect("pending lock") = files;
        self.applied_files.lock().expect("applied lock").clear();
    }

    /// Handle `select <n>` — pick a proposal candidate and transition the
    /// pipeline through Planned → Previewed.  Phase 1C.5 §7.1.
    fn select_candidate(&self, request: &CoreRequest, input: &str) -> CoreResponse {
        // §5.2 制約: select requires Proposed state
        if request.context.pipeline_state != PipelineState::Proposed {
            trace_unsupported_operation(input, "Select", None, "requires Proposed state");
            return error_response("ExecutionError", "Cannot select in current state");
        }

        // §9.2 Proposal未存在
        let Some(proposals) = request.context.current_proposals.as_ref() else {
            return error_response("ExecutionError", "No active proposal");
        };
        if proposals.is_empty() {
            return error_response("ExecutionError", "No active proposal");
        }

        // §3.1 parse 1-based index
        let index_str = input.strip_prefix("select ").map(str::trim).unwrap_or("");
        let index: usize = match index_str.parse::<usize>() {
            Ok(n) if n >= 1 => n,
            _ => return error_response("ExecutionError", "Invalid selection index"),
        };

        // §9.1 bound check
        let Some(candidate) = proposals.get(index - 1) else {
            return error_response("ExecutionError", "Invalid selection index");
        };

        // §11 IR-TRACE
        trace_ir!(
            TraceLevel::Basic,
            "SELECT",
            format!("candidate_id={}", candidate.id)
        );

        // §6 candidate → execution plan
        let plan = match candidate_to_execution_plan(candidate) {
            Ok(plan) => plan,
            Err(err) => return error_response("ValidationError", &err),
        };

        trace_ir!(
            TraceLevel::Basic,
            "PLAN",
            format!("steps={}", plan.steps.join(", "))
        );

        // Build preview diff from already-stored pending files (§10.1 preview必須)
        let pending = self.pending_files.lock().expect("pending lock").clone();
        let preview = if pending.is_empty() {
            vec!["(no pending files)".to_string()]
        } else {
            preview_lines(&pending)
        };

        // Emit: Plan → Pipeline::Planned → Preview → Result → Pipeline::Previewed → Next
        // The double Pipeline emission walks through each required state step.  §5.2
        CoreResponse {
            events: vec![
                CoreEvent::Plan {
                    steps: std::iter::once(format!("Selected: {}", candidate.summary))
                        .chain(plan.steps.iter().cloned())
                        .collect(),
                },
                CoreEvent::Pipeline {
                    state: PipelineState::Planned.label().to_string(),
                },
                CoreEvent::Preview { diff: preview },
                CoreEvent::Result {
                    message: format!("Selected: {}", candidate.summary),
                },
                CoreEvent::Pipeline {
                    state: PipelineState::Previewed.label().to_string(),
                },
                CoreEvent::Next {
                    actions: vec!["apply".to_string()],
                },
            ],
            status: ExecutionStatus::Planned,
            design: None,
        }
    }
}

fn core_events_from_strategy(output: &StrategyOutput) -> Vec<CoreEvent> {
    let mut events = Vec::new();

    if !output.selected_plan.build_plan.build_commands.is_empty()
        || !output.selected_plan.test_plan.test_commands.is_empty()
    {
        let mut steps = output
            .selected_plan
            .build_plan
            .build_commands
            .iter()
            .map(|cmd| format!("build: {cmd}"))
            .collect::<Vec<_>>();
        steps.extend(
            output
                .selected_plan
                .test_plan
                .test_commands
                .iter()
                .map(|cmd| format!("test: {cmd}")),
        );
        events.push(CoreEvent::Plan { steps });
    }

    for attempt in &output.strategy_trace.attempts {
        events.push(CoreEvent::Editing {
            target: format!("{:?}", attempt.strategy_kind),
            action: if attempt.success {
                "accepted execution plan".to_string()
            } else {
                "retry required".to_string()
            },
            reason: attempt
                .failure_context
                .as_ref()
                .map(|failure| format!("{:?}", failure.error)),
        });
    }
    events
}

fn append_clear_intent_runtime_clarification_events(
    events: &mut Vec<CoreEvent>,
    intent: &Intent,
    clarification: &runtime_core::Clarification,
) -> bool {
    if requires_clarification(intent) {
        return false;
    }

    let target = intent
        .file
        .clone()
        .or_else(|| intent.symbol.clone())
        .or_else(|| intent.target.clone());
    let Some(target) = target else {
        return false;
    };
    let action = format!("{:?}", intent.action);

    trace_ir!(
        TraceLevel::Basic,
        "CLARIFICATION_BYPASSED",
        format!(
            "target={target}, action={action}, runtime_message={}",
            clarification.message
        )
    );

    events.push(CoreEvent::Plan {
        steps: vec![format!("{} {}", action.to_ascii_lowercase(), target)],
    });
    events.push(CoreEvent::Pipeline {
        state: PipelineState::Planned.label().to_string(),
    });
    events.push(CoreEvent::Execution {
        step: format!("execute {} on {}", action.to_ascii_lowercase(), target),
    });
    events.push(CoreEvent::Result {
        message: "core execution completed".to_string(),
    });

    true
}

fn design_document_from_core_result(
    result: &runtime_core::stable_v03::RuntimeResult,
    strategy_output: &StrategyOutput,
    previous: Option<&DesignDocument>,
) -> DesignDocument {
    let version = previous
        .map(|doc| doc.version.saturating_add(1))
        .unwrap_or(1);
    let mut reason_units = Vec::new();
    if let Some(trace) = &result.reasoning_trace {
        for step in &trace.steps {
            reason_units.push(ReasonUnit {
                id: format!("reason-depth-{}", step.depth),
                title: format!("depth {}", step.depth),
                summary: format!(
                    "beam={} candidates={} pruned={} recall_hits={}",
                    step.beam_width, step.candidates, step.pruned, step.recall_hits
                ),
            });
        }
    }
    if reason_units.is_empty() {
        reason_units.push(ReasonUnit {
            id: "strategy".to_string(),
            title: "strategy".to_string(),
            summary: strategy_output.strategy_trace.final_outcome.to_string(),
        });
    }

    let functions = result
        .project_layout
        .files
        .iter()
        .take(12)
        .map(|file| file.path.clone())
        .collect::<Vec<_>>();

    DesignDocument::new(
        version,
        reason_units,
        StructureTree {
            module: result.project_layout.root_dir.clone(),
            functions,
        },
        vec![
            Constraint {
                text: "Validation passed before design reflection".to_string(),
            },
            Constraint {
                text: format!(
                    "strategy outcome: {}",
                    strategy_output.strategy_trace.final_outcome
                ),
            },
        ],
    )
}

fn trace_enabled(level: TraceLevel) -> bool {
    TRACE_LEVEL >= level && TRACE_LEVEL != TraceLevel::Off
}

fn trace_event(stage: &str, data: serde_json::Value) -> CoreEvent {
    let rendered = ir_trace_json(stage, data);
    trace_ir!(TraceLevel::Basic, stage, rendered);
    CoreEvent::Debug {
        message: format!("[DETAIL]\n[{stage}] {rendered}"),
    }
}

fn ir_trace_json(stage: &str, data: serde_json::Value) -> String {
    json!({
        "stage": stage,
        "data": data,
        "timestamp": timestamp_millis(),
    })
    .to_string()
}

fn timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn ir_plan_json(plan: &strategy_engine::CodeIrProgram) -> serde_json::Value {
    let steps = execution_steps(plan);
    json!({
        "plan_id": checksum_bytes(format!("{plan:?}").as_bytes()),
        "steps_count": steps.len(),
        "steps": steps,
        "unresolved": plan.build_plan.build_commands.is_empty()
            && plan.run_plan.run_commands.is_empty()
            && plan.test_plan.test_commands.is_empty(),
    })
}

fn execution_plan_json(plan: &strategy_engine::CodeIrProgram) -> serde_json::Value {
    json!({
        "language": format!("{:?}", plan.language),
        "framework": plan.framework,
        "resolved_target": plan.project_root.display().to_string(),
        "constraints": {
            "manifest": plan.dependency_plan.manifest_file,
            "dependencies": plan.dependency_plan.dependencies.len(),
        },
        "execution_steps": execution_steps(plan),
    })
}

fn execution_steps(plan: &strategy_engine::CodeIrProgram) -> Vec<serde_json::Value> {
    let mut steps = Vec::new();
    for command in &plan.dependency_plan.install_commands {
        steps.push(json!({
            "op": "InstallDependency",
            "target": plan.dependency_plan.manifest_file,
            "command": command,
        }));
    }
    for command in &plan.build_plan.build_commands {
        steps.push(json!({
            "op": "Build",
            "target": plan.project_root.display().to_string(),
            "command": command,
        }));
    }
    for command in &plan.run_plan.run_commands {
        steps.push(json!({
            "op": "Run",
            "target": plan.project_root.display().to_string(),
            "command": command,
        }));
    }
    for command in &plan.test_plan.test_commands {
        steps.push(json!({
            "op": "Test",
            "target": plan.project_root.display().to_string(),
            "command": command,
        }));
    }
    steps
}

fn candidates_json_from_strategy(output: &StrategyOutput) -> serde_json::Value {
    let candidates = output
        .strategy_trace
        .attempts
        .iter()
        .map(|attempt| {
            json!({
                "kind": attempt.strategy_kind.to_string(),
                "score": if attempt.success { 1.0 } else { 0.0 },
                "expected_gain": if attempt.success { 1.0 } else { 0.0 },
                "risk": if attempt.success { 0.0 } else { 1.0 },
                "attempt": attempt.attempt_index,
            })
        })
        .collect::<Vec<_>>();
    json!({
        "count": candidates.len(),
        "candidates": candidates,
        "empty": candidates.is_empty(),
    })
}

fn selected_json_from_strategy(output: &StrategyOutput) -> serde_json::Value {
    let selected = output.strategy_trace.attempts.last();
    json!({
        "selected_strategy": selected
            .map(|attempt| attempt.strategy_kind.to_string())
            .unwrap_or_else(|| "none".to_string()),
        "selection_reason": if output.success {
            "successful attempt"
        } else {
            "no successful attempt"
        },
        "score": selected.map(|attempt| if attempt.success { 1.0 } else { 0.0 }),
    })
}

fn execution_result_json(output: &StrategyOutput) -> serde_json::Value {
    let attempts = output
        .strategy_trace
        .attempts
        .iter()
        .map(|attempt| {
            json!({
                "attempt": attempt.attempt_index,
                "status": if attempt.success { "success" } else { "failure" },
                "stdout": attempt.stdout,
                "stderr": attempt.stderr,
                "effects": {
                    "strategy": attempt.strategy_kind.to_string(),
                    "plan_checksum": attempt.plan_checksum.to_string(),
                },
                "error": attempt
                    .failure_context
                    .as_ref()
                    .map(|failure| format!("{:?}", failure.error)),
            })
        })
        .collect::<Vec<_>>();
    json!({
        "status": if output.success { "success" } else { "failure" },
        "outputs": attempts,
        "effects": {
            "selected_plan": checksum_bytes(format!("{:?}", output.selected_plan).as_bytes()),
        },
        "error": if output.success {
            serde_json::Value::Null
        } else {
            json!(output.strategy_trace.final_outcome.to_string())
        },
    })
}

fn trace_unsupported_operation(step_id: &str, op: &str, target: Option<&str>, reason: &str) {
    let data = ir_trace_json(
        "ERROR",
        json!({
            "kind": "Unsupported operation",
            "step_id": step_id,
            "op": op,
            "target": target,
            "reason": reason,
        }),
    );
    trace_ir!(TraceLevel::Error, "ERROR", data);
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingFile {
    path: String,
    content: String,
    content_checksum: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AppliedFile {
    path: String,
    target: PathBuf,
    backup: Option<Vec<u8>>,
    before_checksum: Option<String>,
    after_checksum: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct GitSnapshot {
    modified: Vec<String>,
    staged: Vec<String>,
    head: String,
}

impl GitSnapshot {
    fn from_applied(applied: &[AppliedFile]) -> Self {
        Self {
            modified: applied.iter().map(|file| file.path.clone()).collect(),
            staged: Vec::new(),
            head: String::new(),
        }
    }
}

// ── Select / candidate conversion ────────────────────────────────────────────

/// Internal plan generated from a selected `ExecutionPlanCandidate`.
/// Phase 1C.5 §6.
struct SelectionPlan {
    /// Human-readable step labels derived from the candidate's ops.
    steps: Vec<String>,
}

/// Convert a candidate to an internal `SelectionPlan`.
///
/// Phase 1C.5 §6.1–§6.3
/// Returns `Err` for empty steps or unresolved (empty) target files.
fn candidate_to_execution_plan(
    candidate: &ExecutionPlanCandidate,
) -> Result<SelectionPlan, String> {
    if candidate.steps.is_empty() {
        return Err("candidate has no steps".to_string());
    }
    if let Some(ref target) = candidate.target {
        if target.file.is_empty() {
            return Err("unresolved target file".to_string());
        }
    }
    let steps: Vec<String> = candidate.steps.iter().map(|op| op.label()).collect();
    Ok(SelectionPlan { steps })
}

fn error_response(kind: &str, message: &str) -> CoreResponse {
    CoreResponse {
        events: vec![CoreEvent::Error {
            message: format!("{kind}: {message}"),
        }],
        status: ExecutionStatus::Failed,
        design: None,
    }
}

fn preview_lines(files: &[PendingFile]) -> Vec<String> {
    files
        .iter()
        .flat_map(|file| {
            vec![
                format!("--- {}", file.path),
                format!("+++ {}", file.path),
                format!("+{} bytes", file.content.len()),
            ]
        })
        .collect()
}

fn resolve_repo_file(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.is_absolute() {
        return Err("absolute paths are rejected".to_string());
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("parent directory paths are rejected".to_string());
    }
    Ok(root.join(path))
}

fn validate_git_add_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("git add requires one explicit file path".to_string());
    }
    if path == "." {
        return Err("git add . is rejected".to_string());
    }
    if path.split_whitespace().count() != 1 {
        return Err("git add requires exactly one file path".to_string());
    }
    if path.contains('*') || path.contains('?') || path.contains('[') {
        return Err("git add rejects glob patterns".to_string());
    }
    resolve_repo_file(Path::new("."), path).map(|_| ())
}

fn is_forbidden_command(lower: &str) -> bool {
    lower == "git push"
        || lower.starts_with("git push ")
        || lower == "git reset"
        || lower.starts_with("git reset ")
        || lower == "rm -rf"
        || lower.starts_with("rm -rf ")
}

fn checksum_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn restore_applied(files: &[AppliedFile]) {
    for file in files.iter().rev() {
        match &file.backup {
            Some(content) => {
                let _ = fs::write(&file.target, content);
            }
            None => {
                let _ = fs::remove_file(&file.target);
            }
        }
    }
}

fn run_git(root: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|err| format!("failed to run git: {err}"))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn sync_pipeline_with_git(root: &Path) -> Result<GitSnapshot, String> {
    Ok(GitSnapshot {
        modified: git_lines(root, &["diff", "--name-only"])?,
        staged: git_lines(root, &["diff", "--cached", "--name-only"])?,
        head: run_git(root, &["rev-parse", "--short", "HEAD"])
            .map(|head| head.trim().to_string())
            .unwrap_or_default(),
    })
}

fn git_lines(root: &Path, args: &[&str]) -> Result<Vec<String>, String> {
    Ok(run_git(root, args)?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use strategy_engine::{ExecutionOp, ExecutionPlanCandidate};

    fn request(input: &str) -> CoreRequest {
        CoreRequest::new(
            input.to_string(),
            PathBuf::from("."),
            PipelineState::Idle,
            None,
            None,
        )
    }

    #[test]
    fn ambiguous_input_returns_proposal() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("fix parser bug"));

        assert_eq!(response.status, ExecutionStatus::Proposed);
        assert!(response.events.iter().any(
            |event| matches!(event, CoreEvent::Proposal { candidates } if !candidates.is_empty())
        ));
        assert!(!response.events.iter().any(|event| matches!(event, CoreEvent::Thinking { summary } if summary == "strategy execution started")));
    }

    #[test]
    fn clear_input_returns_plan_and_result() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("refactor parser.rs"));

        assert_eq!(response.status, ExecutionStatus::Executed);
        assert!(
            response
                .events
                .iter()
                .any(|event| matches!(event, CoreEvent::Plan { .. }))
        );
        assert!(
            response
                .events
                .iter()
                .any(|event| matches!(event, CoreEvent::Execution { .. }))
        );
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Result { message } if message == "core execution completed")));
    }

    #[test]
    fn invalid_input_returns_error() {
        let core = RuntimeCoreBridge::with_defaults();
        let response = core.execute(request("git push"));

        assert_eq!(response.status, ExecutionStatus::Failed);
        assert!(response.events.iter().any(|event| matches!(event, CoreEvent::Error { message } if message.contains("SafetyViolation"))));
    }

    #[test]
    fn candidate_to_plan_requires_non_empty_steps() {
        let empty = ExecutionPlanCandidate {
            id: 1,
            summary: "empty".to_string(),
            steps: vec![],
            target: None,
            expected_effects: vec![],
            risks: vec![],
            confidence: 0.0,
            score: 0.0,
        };
        assert!(candidate_to_execution_plan(&empty).is_err());

        let valid = ExecutionPlanCandidate::from_ops(
            1,
            "build",
            vec![ExecutionOp::RuntimePhase("cargo build".to_string())],
            None,
        );
        assert!(candidate_to_execution_plan(&valid).is_ok());
    }
}
