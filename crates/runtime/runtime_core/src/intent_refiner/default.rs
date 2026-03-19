use crate::intent_refiner::refiner::{
    CoreSlot, OptionalSlot, QualitySlot, SlotMap, SlotSource, SlotValue, SystemSlot,
};

pub fn apply_default(mut slots: SlotMap) -> SlotMap {
    slots
        .system
        .entry(SystemSlot::Runtime)
        .or_insert_with(|| SlotValue::new("tokio".to_string(), 0.6, SlotSource::Default));
    if let Some(language) = slots.core.get(&CoreSlot::Language) {
        let framework = match language.value.as_str() {
            "rust" => Some("axum"),
            "go" => Some("gin"),
            "typescript" => Some("express"),
            _ => None,
        };
        if let Some(framework) = framework {
            slots.core.entry(CoreSlot::Framework).or_insert_with(|| {
                SlotValue::new(framework.to_string(), 0.65, SlotSource::Default)
            });
        }
    }
    slots
        .quality
        .entry(QualitySlot::Determinism)
        .or_insert_with(|| SlotValue::new("stable_v03".to_string(), 0.9, SlotSource::Default));
    slots
        .quality
        .entry(QualitySlot::Performance)
        .or_insert_with(|| SlotValue::new("balanced".to_string(), 0.6, SlotSource::Default));
    slots
        .optional
        .entry(OptionalSlot::ArchitectureStyle)
        .or_insert_with(|| SlotValue::new("layered".to_string(), 0.6, SlotSource::Default));
    slots
        .optional
        .entry(OptionalSlot::Testing)
        .or_insert_with(|| SlotValue::new("enabled".to_string(), 0.6, SlotSource::Default));
    slots
}
