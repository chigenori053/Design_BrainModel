use std::io::IsTerminal;

use crate::runtime::host_loop::run_runtime_loop_stdio;
use crate::runtime::logging::{emit_debug, isolate_tui_logging};
use crate::runtime::runtime_events::DebugLevel;
use crate::runtime::runtime_state::initial_runtime_state;
use crate::tui::model::{TraceStatsViewModel, TraceViewModel, UiPayload};

pub const FAILED_BOOTSTRAP: &str = "FAILED_BOOTSTRAP";
pub const FAILED_RUNTIME: &str = "FAILED_RUNTIME";

pub fn start_runtime_tui() -> Result<(), String> {
    let initial = initial_runtime_state();
    if initial.label() != "IDLE" {
        return Err(format!(
            "{FAILED_BOOTSTRAP}: initial runtime state is not IDLE"
        ));
    }

    let tui_mode = std::io::stdin().is_terminal() && std::io::stdout().is_terminal();
    let _guard = tui_mode.then(isolate_tui_logging);
    let _event = emit_debug("RUNTIME][BOOTSTRAP", "start", DebugLevel::Info);

    let result = if tui_mode {
        crate::tui::run_tui(empty_payload())
    } else {
        run_runtime_loop_stdio()
    };

    match result {
        Ok(()) => {
            let _event = emit_debug("RUNTIME][SHUTDOWN", "complete", DebugLevel::Info);
            Ok(())
        }
        Err(err) => {
            let _event = emit_debug("RUNTIME][LOOP", format!("failed: {err}"), DebugLevel::Error);
            Err(format!("{FAILED_RUNTIME}: {err}"))
        }
    }
}

fn empty_payload() -> UiPayload {
    UiPayload {
        trace: TraceViewModel {
            request_id: "runtime-bootstrap".to_string(),
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
