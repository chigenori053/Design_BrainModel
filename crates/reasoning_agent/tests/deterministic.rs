use memory_space_api::MemoryEngine;
use memory_space_complex::{ComplexField, encode_real_vector, normalize};
use memory_space_core::{MemoryField, MemoryId};
use memory_space_index::LinearIndex;
use reasoning_agent::{ReasoningAgent, ReasoningInput, ReasoningResult};

const REL_EPS: f64 = 1e-6;

fn mem(id: MemoryId, values: &[f64]) -> MemoryField {
    let mut field = encode_real_vector(values);
    normalize(&mut field);
    MemoryField {
        id,
        vector: field.data,
    }
}

fn stable_result_hash(out: &ReasoningResult) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    fn fnv1a(h: &mut u64, bytes: &[u8]) {
        for b in bytes {
            *h ^= *b as u64;
            *h = h.wrapping_mul(0x100000001b3);
        }
    }

    fnv1a(&mut h, &[out.stats.used_recall as u8]);
    fnv1a(&mut h, &out.stats.recall_resonance.to_bits().to_le_bytes());
    fnv1a(&mut h, &out.stats.recall_entropy.to_bits().to_le_bytes());
    fnv1a(
        &mut h,
        &(out.stats.hypotheses_generated as u64).to_le_bytes(),
    );
    fnv1a(&mut h, &(out.stats.simulation_steps as u64).to_le_bytes());
    fnv1a(&mut h, &out.stats.evaluation_score.to_bits().to_le_bytes());
    fnv1a(&mut h, &out.confidence.to_bits().to_le_bytes());
    for z in &out.solution_vector.data {
        fnv1a(&mut h, &z.re.to_bits().to_le_bytes());
        fnv1a(&mut h, &z.im.to_bits().to_le_bytes());
    }
    h
}

fn rel_err(a: f64, b: f64) -> f64 {
    let denom = a.abs().max(b.abs()).max(1.0);
    (a - b).abs() / denom
}

fn field_close(a: &ComplexField, b: &ComplexField, eps: f64) -> bool {
    if a.data.len() != b.data.len() {
        return false;
    }
    a.data.iter().zip(&b.data).all(|(l, r)| {
        rel_err(l.re as f64, r.re as f64) <= eps && rel_err(l.im as f64, r.im as f64) <= eps
    })
}

#[test]
fn multi_run_determinism_1000_fixed_seed() {
    let engine = MemoryEngine::with_memory(
        vec![
            mem(1, &[1.0, 0.0]),
            mem(2, &[0.0, 1.0]),
            mem(3, &[0.6, 0.8]),
        ],
        LinearIndex::new(),
    );
    let agent = ReasoningAgent::with_config(engine, 0.95, 3, 8, 2, 0.5);

    let mut query = encode_real_vector(&[0.6, 0.8]);
    normalize(&mut query);

    let baseline = agent.reason(ReasoningInput {
        semantic_vector: query.clone(),
        context: None,
    });
    let baseline_hash = stable_result_hash(&baseline);

    for _ in 0..1000 {
        let out = agent.reason(ReasoningInput {
            semantic_vector: query.clone(),
            context: None,
        });
        assert_eq!(stable_result_hash(&out), baseline_hash);
    }
}

#[test]
fn floating_point_stability_relative_error_below_1e_6() {
    let engine = MemoryEngine::with_memory(
        vec![
            mem(1, &[1.0, 0.0]),
            mem(2, &[0.2, 0.98]),
            mem(3, &[0.8, 0.2]),
        ],
        LinearIndex::new(),
    );
    let agent = ReasoningAgent::with_config(engine, 0.95, 3, 8, 3, 0.5);

    let mut query = encode_real_vector(&[0.45, 0.89]);
    normalize(&mut query);

    let a = agent.reason(ReasoningInput {
        semantic_vector: query.clone(),
        context: None,
    });
    let b = agent.reason(ReasoningInput {
        semantic_vector: query,
        context: None,
    });

    assert!(rel_err(a.confidence, b.confidence) < REL_EPS);
    assert!(rel_err(a.stats.evaluation_score, b.stats.evaluation_score) < REL_EPS);
    assert!(field_close(&a.solution_vector, &b.solution_vector, REL_EPS));
}

#[test]
fn order_invariance_with_shuffled_input() {
    let a = vec![
        mem(1, &[1.0, 0.0]),
        mem(2, &[0.0, 1.0]),
        mem(3, &[0.7, 0.3]),
    ];
    let b = vec![
        mem(3, &[0.7, 0.3]),
        mem(1, &[1.0, 0.0]),
        mem(2, &[0.0, 1.0]),
    ];

    let agent_a = ReasoningAgent::with_config(
        MemoryEngine::with_memory(a, LinearIndex::new()),
        0.95,
        3,
        8,
        2,
        0.5,
    );
    let agent_b = ReasoningAgent::with_config(
        MemoryEngine::with_memory(b, LinearIndex::new()),
        0.95,
        3,
        8,
        2,
        0.5,
    );

    let mut query = encode_real_vector(&[1.0, 0.0]);
    normalize(&mut query);

    let out_a = agent_a.reason(ReasoningInput {
        semantic_vector: query.clone(),
        context: None,
    });
    let out_b = agent_b.reason(ReasoningInput {
        semantic_vector: query,
        context: None,
    });

    assert_eq!(stable_result_hash(&out_a), stable_result_hash(&out_b));
}
