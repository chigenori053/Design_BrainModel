use memory_space_api::MemoryEngine;
use memory_space_complex::{ComplexField, encode_real_vector, normalize};
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

fn field_norm(v: &ComplexField) -> f64 {
    v.data
        .iter()
        .map(|z| z.norm_sqr() as f64)
        .sum::<f64>()
        .sqrt()
}

#[test]
fn recall_reasoning_transition_strong_weak_none() {
    let strong_engine = MemoryEngine::with_memory(vec![mem(1, &[1.0, 0.0])], LinearIndex::new());
    let strong_agent = ReasoningAgent::with_config(strong_engine, 0.6, 1, 8, 2, 0.5);
    let mut strong_query = encode_real_vector(&[1.0, 0.0]);
    normalize(&mut strong_query);
    let strong_out = strong_agent.reason(ReasoningInput {
        semantic_vector: strong_query,
        context: None,
    });
    assert!(strong_out.stats.used_recall);
    assert_eq!(strong_out.stats.hypotheses_generated, 0);
    assert_eq!(strong_out.stats.simulation_steps, 0);

    let weak_engine = MemoryEngine::with_memory(vec![mem(1, &[1.0, 0.0])], LinearIndex::new());
    let weak_agent = ReasoningAgent::with_config(weak_engine, 0.95, 1, 8, 2, 0.5);
    let mut weak_query = encode_real_vector(&[0.7, 0.3]);
    normalize(&mut weak_query);
    let weak_out = weak_agent.reason(ReasoningInput {
        semantic_vector: weak_query,
        context: None,
    });
    assert!(!weak_out.stats.used_recall);
    assert!(weak_out.stats.hypotheses_generated > 0);

    let none_engine = MemoryEngine::new(LinearIndex::new());
    let none_agent = ReasoningAgent::with_config(none_engine, 0.6, 1, 8, 2, 0.5);
    let mut none_query = encode_real_vector(&[0.2, 0.8]);
    normalize(&mut none_query);
    let none_out = none_agent.reason(ReasoningInput {
        semantic_vector: none_query,
        context: None,
    });
    assert!(!none_out.stats.used_recall);
    assert!(none_out.stats.hypotheses_generated > 0);
}

#[test]
fn hypotheses_are_bounded_by_max_k() {
    let engine = MemoryEngine::new(LinearIndex::new());
    let max_k = 3usize;
    let agent = ReasoningAgent::with_config(engine, 0.99, 1, max_k, 2, 0.5);

    let mut query = encode_real_vector(&[0.2, 0.8]);
    normalize(&mut query);
    let out = agent.reason(ReasoningInput {
        semantic_vector: query,
        context: None,
    });
    assert!(out.stats.hypotheses_generated <= max_k);
}

#[test]
fn simulation_state_norm_is_bounded() {
    let engine = MemoryEngine::new(LinearIndex::new());
    let agent = ReasoningAgent::with_config(engine, 0.99, 1, 8, 3, 0.5);

    let mut query = encode_real_vector(&[3.0, 4.0]);
    normalize(&mut query);
    let out = agent.reason(ReasoningInput {
        semantic_vector: query,
        context: None,
    });
    assert!(field_norm(&out.solution_vector) <= 1.000001);
}
