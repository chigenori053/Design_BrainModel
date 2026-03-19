use crate::intent_refiner::refiner::SlotMap;

pub fn merge(rule: SlotMap, inferred: SlotMap) -> SlotMap {
    let mut merged = inferred;
    merged.core.extend(rule.core);
    merged.system.extend(rule.system);
    merged.quality.extend(rule.quality);
    merged.optional.extend(rule.optional);
    merged
}
