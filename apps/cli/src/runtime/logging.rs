use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
#[cfg(test)]
use std::sync::{Mutex, MutexGuard};

use crate::runtime::runtime_events::{DebugEvent, DebugLevel, RuntimeEvent};

static TUI_LOGGING_ISOLATED: AtomicBool = AtomicBool::new(false);
static TUI_SURFACE_ACTIVE_COUNT: AtomicUsize = AtomicUsize::new(0);
static STDOUT_WRITE_DETECTED: AtomicBool = AtomicBool::new(false);
static STDERR_WRITE_DETECTED: AtomicBool = AtomicBool::new(false);
#[cfg(test)]
static TERMINAL_OUTPUT_TEST_LOCK: Mutex<()> = Mutex::new(());

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

pub fn enter_tui_surface() {
    TUI_SURFACE_ACTIVE_COUNT.fetch_add(1, Ordering::SeqCst);
    reset_terminal_write_detection();
}

pub fn leave_tui_surface() {
    let _ = TUI_SURFACE_ACTIVE_COUNT.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
        Some(count.saturating_sub(1))
    });
}

pub fn tui_surface_active() -> bool {
    TUI_SURFACE_ACTIVE_COUNT.load(Ordering::SeqCst) > 0
}

pub fn stdout_write_detected() -> bool {
    STDOUT_WRITE_DETECTED.load(Ordering::SeqCst)
}

pub fn stderr_write_detected() -> bool {
    STDERR_WRITE_DETECTED.load(Ordering::SeqCst)
}

pub fn record_stdout_write() {
    if tui_surface_active() {
        STDOUT_WRITE_DETECTED.store(true, Ordering::SeqCst);
    }
}

pub fn record_stderr_write() {
    if tui_surface_active() {
        STDERR_WRITE_DETECTED.store(true, Ordering::SeqCst);
    }
}

pub fn reset_terminal_write_detection() {
    STDOUT_WRITE_DETECTED.store(false, Ordering::SeqCst);
    STDERR_WRITE_DETECTED.store(false, Ordering::SeqCst);
}

pub fn assert_alternate_screen_exclusive_output() -> Result<(), String> {
    if stdout_write_detected() {
        return Err("stdout write detected while TUI surface active".to_string());
    }
    if stderr_write_detected() {
        return Err("stderr write detected while TUI surface active".to_string());
    }
    Ok(())
}

#[cfg(test)]
pub fn terminal_output_test_lock() -> MutexGuard<'static, ()> {
    TERMINAL_OUTPUT_TEST_LOCK
        .lock()
        .expect("terminal output lock")
}

pub fn emit_debug(source: &str, message: impl Into<String>, level: DebugLevel) -> RuntimeEvent {
    let event = RuntimeEvent::Debug(DebugEvent {
        timestamp: timestamp_millis(),
        source: source.to_string(),
        message: message.into(),
        level,
    });
    if !tui_logging_isolated() && !tui_surface_active() {
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

    #[test]
    fn runtime_logs_never_touch_terminal_surface() {
        let _lock = terminal_output_test_lock();
        enter_tui_surface();
        let _guard = isolate_tui_logging();

        let _event = emit_debug("TEST", "hidden", DebugLevel::Info);

        assert!(tui_surface_active());
        assert!(!stdout_write_detected());
        assert!(!stderr_write_detected());
        assert_alternate_screen_exclusive_output().expect("exclusive output");
        leave_tui_surface();
    }

    #[test]
    fn no_external_terminal_mutation() {
        let _lock = terminal_output_test_lock();
        enter_tui_surface();
        assert!(assert_alternate_screen_exclusive_output().is_ok());

        record_stdout_write();
        assert!(stdout_write_detected());
        assert!(assert_alternate_screen_exclusive_output().is_err());

        reset_terminal_write_detection();
        record_stderr_write();
        assert!(stderr_write_detected());
        assert!(assert_alternate_screen_exclusive_output().is_err());

        leave_tui_surface();
        reset_terminal_write_detection();
    }
}
