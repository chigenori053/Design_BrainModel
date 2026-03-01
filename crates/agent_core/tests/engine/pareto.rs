use core_types::ObjectiveVector;

#[test]
fn dominates_is_strict_and_maximize_based() {
    let a = ObjectiveVector {
        f_struct: 0.8,
        f_field: 0.8,
        f_risk: 0.8,
        f_shape: 0.8,
    };
    let b = ObjectiveVector {
        f_struct: 0.7,
        f_field: 0.8,
        f_risk: 0.8,
        f_shape: 0.8,
    };
    assert!(agent_core::dominates(&a, &b));
    assert!(!agent_core::dominates(&b, &a));
}
