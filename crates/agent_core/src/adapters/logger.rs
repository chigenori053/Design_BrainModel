use std::sync::{Arc, Mutex};

use crate::domain::TelemetryEvent;
use crate::ports::TelemetryPort;

#[derive(Clone, Default, Debug)]
pub struct LoggerTelemetry {
    events: Arc<Mutex<Vec<TelemetryEvent>>>,
}

impl LoggerTelemetry {
    pub fn take(&self) -> Vec<TelemetryEvent> {
        let mut guard = self.events.lock().expect("telemetry mutex poisoned");
        std::mem::take(&mut *guard)
    }
}

impl TelemetryPort for LoggerTelemetry {
    fn emit(&self, event: &TelemetryEvent) {
        self.events
            .lock()
            .expect("telemetry mutex poisoned")
            .push(event.clone());
    }
}
