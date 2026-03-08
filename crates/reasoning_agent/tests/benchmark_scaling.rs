use std::time::Instant;

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

fn unit_vec(seed: usize) -> [f64; 2] {
    let angle = (seed as f64) * 0.017453292519943295;
    [angle.cos(), angle.sin()]
}

fn build_memory_space(n: usize) -> ReasoningAgent<LinearIndex> {
    let mut bank = Vec::with_capacity(n);
    for i in 0..n {
        let v = unit_vec(i + 1);
        bank.push(mem((i + 1) as u64, &v));
    }
    let engine = MemoryEngine::with_memory(bank, LinearIndex::new());
    ReasoningAgent::with_config(engine, 0.9, 1, 8, 2, 10.0)
}

fn random_query_vector() -> memory_space_complex::ComplexField {
    let mut query = encode_real_vector(&unit_vec(42_424));
    normalize(&mut query);
    query
}

#[test]
#[ignore]
fn memory_space_scaling_recall_latency_budget() {
    // NOTE: Keep this out of CI until ANN index is introduced.
    // After ANN rollout, promote this budget test to CI performance gate.
    let agent = build_memory_space(10_000);
    let query = random_query_vector();

    let start = Instant::now();
    let _out = agent.reason(ReasoningInput {
        semantic_vector: query,
        context: None,
    });
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() <= 10,
        "recall latency budget exceeded: {:?}",
        elapsed
    );
}
