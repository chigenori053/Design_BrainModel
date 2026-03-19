use memory_space_phase14::stable_v03::{MemoryEngine, MemoryQuery};

use crate::intent_refiner::refiner::{ChatContext, SlotMap, SlotSource};
use crate::intent_refiner::rule_engine::RuleEngine;
use crate::intent_refiner::tokenizer::Tokenizer;

pub fn apply_memory(
    mut slots: SlotMap,
    input: &str,
    context: &ChatContext,
    memory: &dyn MemoryEngine,
) -> SlotMap {
    let tokenizer = Tokenizer;

    if let Some(previous) = &context.last_slots {
        apply_slot_map(&mut slots, previous, SlotSource::Memory);
    }

    if let Some(previous_input) = context.history.last() {
        let engine = RuleEngine;
        let history_tokens = tokenizer.tokenize(previous_input);
        let extracted = engine.extract(&history_tokens);
        apply_slot_map(&mut slots, &extracted, SlotSource::Memory);
    }
    let query = MemoryQuery {
        text: input.to_string(),
        tags: tokenizer.tokenize(input),
        limit: 1,
    };
    let records = memory.retrieve(query);
    if let Some(record) = records.first() {
        let engine = RuleEngine;
        let mut tokens = tokenizer.tokenize(&record.text);
        tokens.extend(record.tags.iter().map(|tag| tag.to_ascii_lowercase()));
        let extracted = engine.extract(&tokens);
        apply_slot_map(&mut slots, &extracted, SlotSource::Memory);
    }

    slots
}

fn apply_slot_map(target: &mut SlotMap, source: &SlotMap, source_kind: SlotSource) {
    for (key, value) in &source.core {
        target.core.entry(*key).or_insert_with(|| {
            let mut next = value.clone();
            next.source = source_kind;
            next
        });
    }
    for (key, value) in &source.system {
        target.system.entry(*key).or_insert_with(|| {
            let mut next = value.clone();
            next.source = source_kind;
            next
        });
    }
    for (key, value) in &source.quality {
        target.quality.entry(*key).or_insert_with(|| {
            let mut next = value.clone();
            next.source = source_kind;
            next
        });
    }
    for (key, value) in &source.optional {
        target.optional.entry(*key).or_insert_with(|| {
            let mut next = value.clone();
            next.source = source_kind;
            next
        });
    }
}
