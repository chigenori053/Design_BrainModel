use runtime_core::stable_v03::RuntimeResult;
use runtime_core::{ChatContext, Clarification, SlotMap, SlotValue};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChatSession {
    pub history: Vec<String>,
    pub slot_state: Option<SlotMap>,
    pub pending_clarification: Option<Clarification>,
}

impl ChatSession {
    pub fn new() -> Self {
        Self {
            history: vec![],
            slot_state: None,
            pending_clarification: None,
        }
    }

    pub fn to_context(&self) -> ChatContext {
        ChatContext {
            history: self.history.clone(),
            last_slots: self.slot_state.clone(),
        }
    }

    pub fn update_success(&mut self, input: &str, result: &RuntimeResult) {
        self.history.push(input.to_string());
        if let Some(trace) = &result.intent_trace {
            self.slot_state = Some(trace.final_slots.clone());
        }
        self.resolve_clarification();
    }

    pub fn update_pending(
        &mut self,
        input: &str,
        merged_slots: Option<SlotMap>,
        clarification: Clarification,
    ) {
        self.history.push(input.to_string());
        self.slot_state = merged_slots;
        self.update_clarification(clarification);
    }

    pub fn update_clarification(&mut self, clarification: Clarification) {
        self.pending_clarification = Some(clarification);
    }

    pub fn resolve_clarification(&mut self) {
        self.pending_clarification = None;
    }

    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

pub fn merge_slots(prev: &SlotMap, new: &SlotMap) -> SlotMap {
    let mut merged = prev.clone();
    merge_slot_values(&mut merged.core, &new.core);
    merge_slot_values(&mut merged.system, &new.system);
    merge_slot_values(&mut merged.quality, &new.quality);
    merge_slot_values(&mut merged.optional, &new.optional);
    merged
}

fn merge_slot_values<K>(
    target: &mut std::collections::HashMap<K, SlotValue>,
    incoming: &std::collections::HashMap<K, SlotValue>,
) where
    K: std::cmp::Eq + std::hash::Hash + Copy,
{
    for (slot, value) in incoming {
        if value.value.trim().is_empty() {
            continue;
        }
        target.insert(*slot, value.clone());
    }
}
