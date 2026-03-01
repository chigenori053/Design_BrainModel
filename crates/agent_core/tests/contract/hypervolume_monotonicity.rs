#[test]
fn hypervolume_monotonicity() {
    let a = vec![[0.2, 0.2, 0.2, 0.2]];
    let b = vec![[0.2, 0.2, 0.2, 0.2], [0.8, 0.8, 0.8, 0.8]];
    let hv_a = agent_core::hv_4d_from_origin_normalized(&a);
    let hv_b = agent_core::hv_4d_from_origin_normalized(&b);
    assert!(hv_b + 1e-12 >= hv_a);
}
