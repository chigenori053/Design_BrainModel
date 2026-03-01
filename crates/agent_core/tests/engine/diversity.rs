#[test]
fn profile_modulation_is_bounded() {
    for s in [-1.0, -0.3, 0.0, 0.5, 1.0] {
        let h = agent_core::profile_modulation(s);
        assert!(h.is_finite());
        assert!((0.0..=2.0).contains(&h));
    }
}
