use crate::intent_refiner::refiner::{
    CoreSlot, OptionalSlot, QualitySlot, SlotMap, SlotSource, SlotValue, SystemSlot,
};

#[derive(Clone, Debug, Default)]
pub struct RuleEngine;

impl RuleEngine {
    pub fn extract(&self, tokens: &[String]) -> SlotMap {
        let mut slots = SlotMap::default();
        for token in tokens {
            match token.as_str() {
                "api" | "rest" | "graphql" | "web" | "ui" => {
                    slots.insert_core(
                        CoreSlot::InterfaceType,
                        SlotValue::new(token.clone(), 1.0, SlotSource::Explicit),
                    );
                }
                "rust" | "go" | "typescript" => {
                    slots.insert_core(
                        CoreSlot::Language,
                        SlotValue::new(token.clone(), 1.0, SlotSource::Explicit),
                    );
                }
                "axum" | "actix" | "gin" | "express" | "next" => {
                    slots.insert_core(
                        CoreSlot::Framework,
                        SlotValue::new(token.clone(), 1.0, SlotSource::Explicit),
                    );
                }
                "postgres" | "mysql" | "sqlite" | "redis" | "db" | "store" => {
                    slots.insert_system(
                        SystemSlot::Runtime,
                        SlotValue::new(token.clone(), 1.0, SlotSource::Explicit),
                    );
                }
                "tokio" | "bun" | "node" => {
                    slots.insert_system(
                        SystemSlot::Runtime,
                        SlotValue::new(token.clone(), 1.0, SlotSource::Explicit),
                    );
                }
                "deterministic" | "determinism" | "stable" => {
                    slots.insert_quality(
                        QualitySlot::Determinism,
                        SlotValue::new("stable_v03".to_string(), 0.95, SlotSource::Explicit),
                    );
                }
                "fast" | "performance" | "latency" => {
                    slots.insert_quality(
                        QualitySlot::Performance,
                        SlotValue::new("balanced".to_string(), 0.8, SlotSource::Explicit),
                    );
                }
                "layered" | "clean" | "hexagonal" => {
                    slots.insert_optional(
                        OptionalSlot::ArchitectureStyle,
                        SlotValue::new(token.clone(), 0.9, SlotSource::Explicit),
                    );
                }
                "test" | "testing" => {
                    slots.insert_optional(
                        OptionalSlot::Testing,
                        SlotValue::new("enabled".to_string(), 0.9, SlotSource::Explicit),
                    );
                }
                _ => {}
            }
        }
        slots
    }
}
