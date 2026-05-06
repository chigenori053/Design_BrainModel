use std::collections::VecDeque;

use crate::runtime::runtime_events::RuntimeEvent;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeEventQueue {
    queue: VecDeque<RuntimeEvent>,
}

impl RuntimeEventQueue {
    pub fn emit(&mut self, event: RuntimeEvent) {
        self.queue.push_back(event);
    }

    pub fn consume(&mut self) -> Option<RuntimeEvent> {
        self.queue.pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &RuntimeEvent> {
        self.queue.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_preserves_event_order() {
        let mut queue = RuntimeEventQueue::default();
        queue.emit(RuntimeEvent::BootstrapStarted);
        queue.emit(RuntimeEvent::LoopStarted);

        assert_eq!(queue.consume(), Some(RuntimeEvent::BootstrapStarted));
        assert_eq!(queue.consume(), Some(RuntimeEvent::LoopStarted));
        assert!(queue.is_empty());
    }
}
