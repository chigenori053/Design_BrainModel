use crate::intent_refiner::refiner::{CoreSlot, SlotMap, SlotSource, SlotValue, SystemSlot};

#[derive(Clone, Debug, Default)]
pub struct InferenceEngine;

impl InferenceEngine {
    pub fn infer(&self, input: &str) -> SlotMap {
        let mut slots = SlotMap::default();
        let contains_api = input.contains("api")
            || input.contains("rest")
            || input.contains("graphql")
            || input.contains("web");

        if contains_api {
            slots.insert_core(
                CoreSlot::InterfaceType,
                SlotValue::new("api".to_string(), 0.85, SlotSource::Inferred),
            );
        }
        if input.contains("rust") || input.contains("axum") || input.contains("tokio") {
            slots.insert_core(
                CoreSlot::Language,
                SlotValue::new("rust".to_string(), 0.8, SlotSource::Inferred),
            );
        }
        if input.contains("go") || input.contains("gin") {
            slots.insert_core(
                CoreSlot::Language,
                SlotValue::new("go".to_string(), 0.8, SlotSource::Inferred),
            );
        }
        if input.contains("typescript") || input.contains("node") || input.contains("express") {
            slots.insert_core(
                CoreSlot::Language,
                SlotValue::new("typescript".to_string(), 0.8, SlotSource::Inferred),
            );
        }
        if input.contains("axum") || input.contains("actix") {
            slots.insert_core(
                CoreSlot::Framework,
                SlotValue::new("axum".to_string(), 0.75, SlotSource::Inferred),
            );
        }
        if input.contains("gin") {
            slots.insert_core(
                CoreSlot::Framework,
                SlotValue::new("gin".to_string(), 0.75, SlotSource::Inferred),
            );
        }
        if input.contains("express") || input.contains("next") {
            slots.insert_core(
                CoreSlot::Framework,
                SlotValue::new("express".to_string(), 0.75, SlotSource::Inferred),
            );
        }
        if input.contains("tokio") {
            slots.insert_system(
                SystemSlot::Runtime,
                SlotValue::new("tokio".to_string(), 0.8, SlotSource::Inferred),
            );
        }
        slots
    }
}
