use memory_space_api::MemoryEngine;
use memory_space_complex::{encode_real_vector, normalize};
use memory_space_core::{MemoryField, MemoryId};
use memory_space_index::LinearIndex;
use reasoning_agent::evaluation::evaluate;

fn mem(id: MemoryId, values: &[f64]) -> MemoryField {
    let mut field = encode_real_vector(values);
    normalize(&mut field);
    MemoryField {
        id,
        vector: field.data,
    }
}

#[test]
fn evaluate_is_deterministic() {
    let engine = MemoryEngine::with_memory(
        vec![mem(1, &[1.0, 0.0]), mem(2, &[0.0, 1.0])],
        LinearIndex::new(),
    );

    let mut state = encode_real_vector(&[1.0, 0.0]);
    normalize(&mut state);

    let a = evaluate(&state, &engine, 2);
    let b = evaluate(&state, &engine, 2);
    assert_eq!(a, b);
}
