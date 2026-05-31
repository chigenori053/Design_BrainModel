use std::sync::{Mutex, OnceLock};

static RENDER_TRACE: OnceLock<Mutex<Vec<&'static str>>> = OnceLock::new();

fn trace_events() -> &'static Mutex<Vec<&'static str>> {
    RENDER_TRACE.get_or_init(|| Mutex::new(Vec::new()))
}

pub fn record(event: &'static str) {
    trace_events().lock().expect("render trace").push(event);
}

#[cfg(test)]
pub fn reset() {
    trace_events().lock().expect("render trace").clear();
}

#[cfg(test)]
pub fn snapshot() -> Vec<&'static str> {
    trace_events().lock().expect("render trace").clone()
}
