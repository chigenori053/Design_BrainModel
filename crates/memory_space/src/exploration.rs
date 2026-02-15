use std::collections::{BTreeMap, BTreeSet};

use crate::types::StateId;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExplorationMemory {
    pub states: BTreeMap<StateId, Vec<f64>>,
    pub transitions: BTreeSet<(StateId, StateId)>,
}

impl ExplorationMemory {
    pub fn new(states: BTreeMap<StateId, Vec<f64>>, transitions: BTreeSet<(StateId, StateId)>) -> Self {
        Self { states, transitions }
    }

    pub fn with_state_registered(&self, state_id: StateId, evaluation: Vec<f64>) -> Self {
        let mut next = self.clone();
        next.states.insert(state_id, evaluation);
        next
    }

    pub fn with_transition_registered(&self, from: StateId, to: StateId) -> Self {
        if !self.states.contains_key(&from) || !self.states.contains_key(&to) {
            return self.clone();
        }

        let mut next = self.clone();
        next.transitions.insert((from, to));
        next
    }
}

#[cfg(test)]
mod tests {
    use crate::{ExplorationMemory, Uuid};

    #[test]
    fn exploration_memory_registers_states_and_transitions() {
        let state_a = Uuid::from_u128(11);
        let state_b = Uuid::from_u128(12);

        let memory = ExplorationMemory::default()
            .with_state_registered(state_a, vec![0.1, 0.2])
            .with_state_registered(state_b, vec![0.3, 0.4])
            .with_transition_registered(state_a, state_b);

        assert_eq!(memory.states.get(&state_a), Some(&vec![0.1, 0.2]));
        assert!(memory.transitions.contains(&(state_a, state_b)));
    }
}
