use crate::{DesignIntent, IntentModel};

#[derive(Clone, Debug, Default)]
pub struct IntentProcessor;

impl IntentProcessor {
    pub fn process(&self, intent: &IntentModel) -> DesignIntent {
        DesignIntent {
            required_components: intent.required_component_types(),
            required_features: intent.requirements.clone(),
            architectural_constraints: intent.constraints.architecture.iter().cloned().collect(),
        }
    }
}
