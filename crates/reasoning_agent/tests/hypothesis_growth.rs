use memory_space_api::MemoryEngine;
use memory_space_complex::{encode_real_vector, normalize};
use memory_space_index::LinearIndex;
use reasoning_agent::{ReasoningAgent, ReasoningInput};

#[test]
fn generated_hypotheses_are_bounded() {
    let engine = MemoryEngine::new(LinearIndex::new());
    let agent = ReasoningAgent::with_config(engine, 0.99, 1, 2, 2, 0.5);

    let mut query = encode_real_vector(&[0.2, 0.8]);
    normalize(&mut query);

    let out = agent.reason(ReasoningInput {
        semantic_vector: query,
        context: None,
    });

    assert!(out.stats.hypotheses_generated <= 2);
}
