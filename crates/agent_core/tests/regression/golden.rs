use core_types::ObjectiveVector;

#[test]
fn golden_scalar_score_matches_legacy_formula() {
    let obj = ObjectiveVector {
        f_struct: 0.77,
        f_field: 0.11,
        f_risk: 0.23,
        f_shape: 0.91,
    };
    let legacy = agent_core::scalar_score(&obj);
    let expected = 0.4 * 0.77 + 0.2 * 0.11 + 0.2 * 0.23 + 0.2 * 0.91;
    assert!((legacy - expected).abs() < 1e-12);
}
