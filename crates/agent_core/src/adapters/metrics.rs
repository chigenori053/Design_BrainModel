use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Debug, Default)]
pub struct MetricsCounter {
    dispatched: Arc<AtomicU64>,
}

impl MetricsCounter {
    pub fn increment_dispatch(&self) {
        self.dispatched.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dispatched(&self) -> u64 {
        self.dispatched.load(Ordering::Relaxed)
    }
}
