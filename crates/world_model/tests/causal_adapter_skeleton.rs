use world_model::WorldStateToCausalAdapter;

#[test]
fn adapter_skeleton_is_available() {
    assert!(WorldStateToCausalAdapter::is_available());
}
