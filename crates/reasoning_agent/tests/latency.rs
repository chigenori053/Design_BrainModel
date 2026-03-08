use std::time::Instant;

use memory_space_api::MemoryEngine;
use memory_space_complex::{encode_real_vector, normalize};
use memory_space_core::MemoryField;
use memory_space_index::LinearIndex;
use reasoning_agent::{ReasoningAgent, ReasoningInput};

#[test]
fn reasoning_latency_is_bounded_for_small_case() {
    let mut bank = Vec::new();
    for id in 0..32u64 {
        let mut field = encode_real_vector(&[1.0, id as f64 / 32.0]);
        normalize(&mut field);
        bank.push(MemoryField {
            id,
            vector: field.data,
        });
    }

    let engine = MemoryEngine::with_memory(bank, LinearIndex::new());
    let agent = ReasoningAgent::with_config(engine, 0.95, 3, 8, 2, 0.3);

    let mut query = encode_real_vector(&[0.2, 0.8]);
    normalize(&mut query);

    let start = Instant::now();
    let _ = agent.reason(ReasoningInput {
        semantic_vector: query,
        context: None,
    });
    let elapsed = start.elapsed().as_millis();

    assert!(elapsed < 1000, "latency too high: {elapsed}ms");
}
