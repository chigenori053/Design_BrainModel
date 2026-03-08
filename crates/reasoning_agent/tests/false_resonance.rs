use memory_space_api::MemoryEngine;
use memory_space_complex::{encode_real_vector, normalize};
use memory_space_core::{MemoryField, MemoryId};
use memory_space_index::LinearIndex;
use reasoning_agent::{ReasoningAgent, ReasoningInput};

fn mem(id: MemoryId, values: &[f64]) -> MemoryField {
    let mut field = encode_real_vector(values);
    normalize(&mut field);
    MemoryField {
        id,
        vector: field.data,
    }
}

#[test]
fn noise_query_does_not_trigger_recall() {
    let engine = MemoryEngine::with_memory(vec![mem(1, &[1.0, 0.0])], LinearIndex::new());
    let agent = ReasoningAgent::with_config(engine, 0.95, 1, 8, 2, 0.5);

    let mut query = encode_real_vector(&[0.0, 1.0]);
    normalize(&mut query);

    let out = agent.reason(ReasoningInput {
        semantic_vector: query,
        context: None,
    });

    assert!(!out.stats.used_recall);
}
