use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeEvent {
    InputAccepted,
    ModalityNormalized,
    MemoryRecallRequested,
    MemoryRecallCompleted,
    HypothesisGenerated,
    TransitionEvaluated,
    ConsistencyScored,
    OutputProduced,
    // Phase9-D: design search events
    SearchStarted,
    CandidateExpanded,
    SimulationStarted,
    SimulationCompleted,
    CandidatePruned,
    CandidateRanked,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeEventBus {
    events: VecDeque<RuntimeEvent>,
}

impl RuntimeEventBus {
    pub fn publish(&mut self, event: RuntimeEvent) {
        self.events.push_back(event);
    }

    pub fn drain(&mut self) -> Vec<RuntimeEvent> {
        self.events.drain(..).collect()
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn events(&self) -> impl Iterator<Item = &RuntimeEvent> {
        self.events.iter()
    }
}
