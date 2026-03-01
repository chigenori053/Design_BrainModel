#[test]
fn hypervolume_bounds_in_unit_cube() {
    let points = vec![[0.1, 0.3, 0.5, 0.7], [0.7, 0.5, 0.3, 0.1]];
    let hv = agent_core::hv_4d_from_origin_normalized(&points);
    assert!((0.0..=1.0).contains(&hv));
}
