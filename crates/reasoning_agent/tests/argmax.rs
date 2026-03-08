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
fn argmax_selection_is_deterministic() {
    let engine = MemoryEngine::with_memory(
        vec![
            mem(1, &[1.0, 0.0]),
            mem(2, &[1.0, 0.0]),
            mem(3, &[0.0, 1.0]),
        ],
        LinearIndex::new(),
    );
    let agent = ReasoningAgent::with_config(engine, 0.99, 3, 8, 2, 0.0);

    let mut query = encode_real_vector(&[1.0, 0.0]);
    normalize(&mut query);

    let input = ReasoningInput {
        semantic_vector: query,
        context: None,
    };

    let a = agent.reason(input.clone());
    let b = agent.reason(input);
    assert_eq!(a.solution_vector, b.solution_vector);
    assert_eq!(a.stats.evaluation_score, b.stats.evaluation_score);
}
