use crate::domain::TelemetryEvent;

pub trait TelemetryPort: Send + Sync {
    fn emit(&self, event: &TelemetryEvent);
}
