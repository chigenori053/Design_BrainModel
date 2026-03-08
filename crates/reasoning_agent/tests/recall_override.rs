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
fn strong_recall_skips_reasoning_pipeline() {
    let recalled = mem(1, &[1.0, 0.0]);
    let engine = MemoryEngine::with_memory(vec![recalled.clone()], LinearIndex::new());
    let agent = ReasoningAgent::with_config(engine, 0.5, 1, 8, 2, 0.5);

    let mut query = encode_real_vector(&[1.0, 0.0]);
    normalize(&mut query);

    let out = agent.reason(ReasoningInput {
        semantic_vector: query,
        context: None,
    });

    assert!(out.stats.used_recall);
    assert_eq!(out.stats.hypotheses_generated, 0);
    assert_eq!(out.stats.simulation_steps, 0);
    assert_eq!(out.solution_vector.data, recalled.vector);
}
