use std::time::Instant;

use memory_space_api::MemoryEngine;
use memory_space_complex::{ComplexField, encode_real_vector, normalize};
use memory_space_core::{MemoryField, MemoryId};
use memory_space_index::LinearIndex;
use reasoning_agent::{ReasoningAgent, ReasoningInput};

const REASONING_LATENCY_BUDGET_MS: u128 = 50;

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
fn memory_space_scaling_recall_latency_growth_is_reasonable() {
    let sizes = [10usize, 100, 1000, 10_000];
    let mut prev_latency = None;

    for &n in &sizes {
        let agent = build_memory_space(n);
        let query = random_query_vector();
        let start = Instant::now();
        let out = agent.reason(ReasoningInput {
            semantic_vector: query,
            context: None,
        });
        let elapsed = start.elapsed();

        assert!(
            (0.0..=1.0).contains(&out.stats.recall_resonance),
            "recall resonance out of bounds at n={n}: {}",
            out.stats.recall_resonance
        );
        assert!(
            out.stats.recall_entropy >= 0.0,
            "recall entropy must be non-negative at n={n}: {}",
            out.stats.recall_entropy
        );

        if let Some(prev) = prev_latency {
            assert!(
                elapsed <= prev * 20,
                "latency growth too large at n={n}: {:?} -> {:?}",
                prev,
                elapsed
            );
        }
        prev_latency = Some(elapsed);
    }
}

#[test]
fn long_reasoning_chain_100_iterations_is_stable() {
    let engine = MemoryEngine::new(LinearIndex::new());
    let agent = ReasoningAgent::with_config(engine, 0.99, 1, 8, 3, 0.5);

    let mut current = encode_real_vector(&[0.6, 0.8]);
    normalize(&mut current);

    let start = Instant::now();
    for _ in 0..100 {
        let out = agent.reason(ReasoningInput {
            semantic_vector: current.clone(),
            context: None,
        });
        assert!(
            field_norm(&out.solution_vector) <= 1.000001,
            "state norm diverged beyond bound"
        );
        assert!(
            out.stats.hypotheses_generated <= 8,
            "hypothesis explosion detected: {}",
            out.stats.hypotheses_generated
        );
        current = out.solution_vector;
    }
    let elapsed = start.elapsed().as_millis();
    let per_iter = elapsed as f64 / 100.0;
    assert!(
        per_iter < REASONING_LATENCY_BUDGET_MS as f64,
        "reasoning latency budget exceeded: {per_iter:.3}ms/iter"
    );
}

#[test]
fn conflicting_memories_limit_false_recall_rate() {
    let engine = MemoryEngine::with_memory(
        vec![
            mem(1, &[1.0, 0.0]),
            mem(2, &[1.0, 0.0]),
            mem(3, &[1.0, 0.0]),
            mem(4, &[0.0, 1.0]),
        ],
        LinearIndex::new(),
    );
    let agent = ReasoningAgent::with_config(engine, 0.8, 3, 8, 2, 0.05);

    let mut false_recalls = 0usize;
    let runs = 100usize;
    for i in 0..runs {
        let v = unit_vec(i + 10_000);
        let mut q = encode_real_vector(&v);
        normalize(&mut q);
        let out = agent.reason(ReasoningInput {
            semantic_vector: q,
            context: None,
        });
        if out.stats.used_recall {
            false_recalls += 1;
        }
    }

    let false_recall_rate = false_recalls as f64 / runs as f64;
    assert!(
        false_recall_rate <= 0.10,
        "false recall rate too high: {false_recall_rate}"
    );
}
