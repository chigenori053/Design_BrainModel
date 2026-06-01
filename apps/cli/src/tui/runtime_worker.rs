use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::thread;

use crate::core::{CoreExecutor, CoreRequest, CoreResponse, RuntimeCoreBridge};

static RUNTIME_TASK_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RuntimeTaskId(pub u64);

impl RuntimeTaskId {
    fn next() -> Self {
        Self(RUNTIME_TASK_COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeStatus {
    Queued,
    Planning,
    Executing,
    Projecting,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRequest {
    pub raw: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTask {
    pub id: RuntimeTaskId,
    pub request: RuntimeRequest,
    pub status: RuntimeStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeResult {
    pub task_id: RuntimeTaskId,
    pub status: RuntimeStatus,
    pub output: String,
    pub response: CoreResponse,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeWorkerEvent {
    Progress {
        task_id: RuntimeTaskId,
        status: RuntimeStatus,
    },
    Result(RuntimeResult),
}

pub fn spawn_runtime_worker(
    core: Arc<RuntimeCoreBridge>,
    request: CoreRequest,
    sender: Sender<RuntimeWorkerEvent>,
) -> RuntimeTask {
    let task = RuntimeTask {
        id: RuntimeTaskId::next(),
        request: RuntimeRequest {
            raw: request.raw.clone(),
        },
        status: RuntimeStatus::Queued,
    };
    let task_id = task.id;
    crate::tui::render_trace::record("[WORKER_SPAWN]");

    thread::spawn(move || {
        crate::tui::render_trace::record("[WORKER_STARTED]");
        crate::tui::render_trace::record(Box::leak(
            format!("[WORKER_RECEIVED_REQUEST] task={}", task_id.0).into_boxed_str(),
        ));
        let _ = sender.send(RuntimeWorkerEvent::Progress {
            task_id,
            status: RuntimeStatus::Planning,
        });
        crate::tui::render_trace::record("[PLANNING]");

        let _ = sender.send(RuntimeWorkerEvent::Progress {
            task_id,
            status: RuntimeStatus::Executing,
        });
        crate::tui::render_trace::record("[EXECUTING]");

        let mut response = core.execute(request);
        crate::tui::render_trace::record("[WORKER_FINISHED]");
        if response.events.is_empty() {
            response.events.push(crate::core::CoreEvent::Error {
                message: "No runtime narrative generated".to_string(),
            });
        }

        let status = if response.status == crate::core::ExecutionStatus::Failed {
            RuntimeStatus::Failed
        } else {
            RuntimeStatus::Completed
        };
        let output = response
            .events
            .iter()
            .map(|event| format!("{event:?}"))
            .collect::<Vec<_>>()
            .join("\n");

        let _ = sender.send(RuntimeWorkerEvent::Result(RuntimeResult {
            task_id,
            status,
            output,
            response,
        }));
        crate::tui::render_trace::record("[WORKER_RESULT]");
    });

    task
}
