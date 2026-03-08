use memory_space_api::MemoryEngine;
use memory_space_complex::{encode_real_vector, normalize};
use memory_space_index::LinearIndex;
use reasoning_agent::{ReasoningAgent, ReasoningInput};

fn field_norm(values: &[memory_space_core::Complex64]) -> f64 {
    values
        .iter()
        .map(|z| z.norm_sqr() as f64)
        .sum::<f64>()
        .sqrt()
}

#[test]
fn simulated_state_norm_is_bounded() {
    let engine = MemoryEngine::new(LinearIndex::new());
    let agent = ReasoningAgent::with_config(engine, 0.99, 1, 8, 3, 0.5);

    let mut query = encode_real_vector(&[3.0, 4.0]);
    normalize(&mut query);

    let out = agent.reason(ReasoningInput {
        semantic_vector: query,
        context: None,
    });

    assert!(field_norm(&out.solution_vector.data) <= 1.000001);
}
