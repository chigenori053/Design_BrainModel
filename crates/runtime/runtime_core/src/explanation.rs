use crate::intent_refiner::{CoreSlot, IntentTrace, SlotSource};
use crate::stable_v03::RuntimeResult;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Explanation {
    pub intent: Vec<SlotExplanation>,
    pub decisions: Vec<DecisionExplanation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotExplanation {
    pub slot: String,
    pub value: String,
    pub source: SlotSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionExplanation {
    pub message: String,
}

pub trait ExplanationBuilder: Send + Sync {
    fn build(&self, trace: &IntentTrace, result: &RuntimeResult) -> Explanation;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultExplanationBuilder;

impl ExplanationBuilder for DefaultExplanationBuilder {
    fn build(&self, trace: &IntentTrace, _result: &RuntimeResult) -> Explanation {
        Explanation {
            intent: explain_slots(trace),
            decisions: explain_decisions(trace),
        }
    }
}

pub fn explain_slots(trace: &IntentTrace) -> Vec<SlotExplanation> {
    let mut items = trace
        .final_slots
        .core
        .iter()
        .map(|(slot, value)| SlotExplanation {
            slot: format!("{slot:?}"),
            value: value.value.clone(),
            source: value.source,
        })
        .collect::<Vec<_>>();
    items.sort_by(|lhs, rhs| {
        lhs.slot
            .cmp(&rhs.slot)
            .then_with(|| lhs.value.cmp(&rhs.value))
    });
    items
}

pub fn explain_decisions(trace: &IntentTrace) -> Vec<DecisionExplanation> {
    let mut decisions = Vec::new();

    if trace.inferred.core.contains_key(&CoreSlot::InterfaceType) {
        decisions.push(DecisionExplanation {
            message: "Interface inferred from keyword 'api'".to_string(),
        });
    }
    if let Some(value) = trace.final_slots.core.get(&CoreSlot::Framework) {
        if value.source == SlotSource::Default {
            decisions.push(DecisionExplanation {
                message: format!(
                    "Framework defaulted from language '{}'",
                    trace
                        .final_slots
                        .core
                        .get(&CoreSlot::Language)
                        .map(|slot| slot.value.as_str())
                        .unwrap_or("unknown")
                ),
            });
        }
    }

    decisions
}

pub fn source_to_message(source: &SlotSource) -> &'static str {
    match source {
        SlotSource::Explicit => "explicitly specified",
        SlotSource::Inferred => "inferred from input",
        SlotSource::Memory => "derived from previous context",
        SlotSource::Default => "default applied",
    }
}
