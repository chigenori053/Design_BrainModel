use std::sync::atomic::{AtomicBool, Ordering};

use crate::runtime::runtime_events::{DebugEvent, DebugLevel, RuntimeEvent};

static TUI_LOGGING_ISOLATED: AtomicBool = AtomicBool::new(false);

pub struct TuiLoggingGuard {
    previous: bool,
}

impl Drop for TuiLoggingGuard {
    fn drop(&mut self) {
        TUI_LOGGING_ISOLATED.store(self.previous, Ordering::SeqCst);
    }
}

pub fn isolate_tui_logging() -> TuiLoggingGuard {
    let previous = TUI_LOGGING_ISOLATED.swap(true, Ordering::SeqCst);
    TuiLoggingGuard { previous }
}

pub fn tui_logging_isolated() -> bool {
    TUI_LOGGING_ISOLATED.load(Ordering::SeqCst)
}

pub fn emit_debug(source: &str, message: impl Into<String>, level: DebugLevel) -> RuntimeEvent {
    let event = RuntimeEvent::Debug(DebugEvent {
        timestamp: timestamp_millis(),
        source: source.to_string(),
        message: message.into(),
        level,
    });
    if !tui_logging_isolated() {
        eprintln!("[{}][{:?}] {}", source, level, event.message());
    }
    event
}

fn timestamp_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tui_logging_guard_restores_previous_state() {
        assert!(!tui_logging_isolated());
        {
            let _guard = isolate_tui_logging();
            assert!(tui_logging_isolated());
        }
        assert!(!tui_logging_isolated());
    }
}
