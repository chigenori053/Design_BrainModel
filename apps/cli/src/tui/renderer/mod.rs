use crate::runtime::event_queue::RuntimeEventQueue;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenderScheduler {
    pending: bool,
}

impl RenderScheduler {
    pub fn observe_queue(&mut self, queue: &RuntimeEventQueue) {
        if !queue.is_empty() {
            self.pending = true;
        }
    }

    pub fn request(&mut self) {
        self.pending = true;
    }

    pub fn take_pending(&mut self) -> bool {
        let pending = self.pending;
        self.pending = false;
        pending
    }
}
