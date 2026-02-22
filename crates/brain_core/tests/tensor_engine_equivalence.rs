use brain_core::tensor::{Tensor, TensorEngine};
use std::hint::black_box;
use std::time::Instant;

fn relative_error(old: f64, new: f64) -> f64 {
    let denom = old.abs().max(1e-12);
    (old - new).abs() / denom
}

#[test]
fn relative_error_threshold_matches_spec_requirement() {
    let old_out = 1.0_f64;
    let new_out = 1.0_f64 + 5e-7;
    assert!(relative_error(old_out, new_out) < 1e-6);
}

#[test]
fn tensor_engine_old_vs_new_numerical_equivalence_gate() {
    let input = Tensor::new(vec![0.1, -0.25, 1.0, 2.0], vec![2, 2]);
    let engine = TensorEngine;

    // Temporary baseline for T-001: old path keeps behavior as direct clone.
    let old_out = input.clone();
    let new_out = engine.run(&input);

    assert_eq!(old_out.shape(), new_out.shape());
    for (old, new) in old_out.values().iter().zip(new_out.values().iter()) {
        assert!(relative_error(*old, *new) < 1e-6);
    }
}

#[test]
fn tensor_engine_perf_smoke_within_five_percent() {
    let input = Tensor::new((0..8192).map(|i| i as f64 * 0.001).collect(), vec![128, 64]);
    let engine = TensorEngine;
    let iterations = 3000;

    let start_old = Instant::now();
    for _ in 0..iterations {
        black_box(input.clone());
    }
    let old_elapsed = start_old.elapsed();

    let start_new = Instant::now();
    for _ in 0..iterations {
        black_box(engine.run(&input));
    }
    let new_elapsed = start_new.elapsed();

    let ratio = new_elapsed.as_secs_f64() / old_elapsed.as_secs_f64();
    assert!(
        ratio <= 1.05,
        "new path regression too high: ratio={ratio:.4}, old={old_elapsed:?}, new={new_elapsed:?}"
    );
}
