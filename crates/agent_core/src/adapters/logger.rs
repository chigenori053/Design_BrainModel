use std::sync::{Arc, Mutex};

use crate::domain::TelemetryEvent;
use crate::ports::TelemetryPort;

#[derive(Clone, Default, Debug)]
pub struct LoggerTelemetry {
    events: Arc<Mutex<Vec<TelemetryEvent>>>,
}

impl LoggerTelemetry {
    pub fn take(&self) -> Vec<TelemetryEvent> {
        match self.events.lock() {
            Ok(mut guard) => std::mem::take(&mut *guard),
            Err(_) => Vec::new(),
        }
    }
}

impl TelemetryPort for LoggerTelemetry {
    fn emit(&self, event: &TelemetryEvent) {
        if let Ok(mut guard) = self.events.lock() {
            guard.push(event.clone());
        }
    }
}
