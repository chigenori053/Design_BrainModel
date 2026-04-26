use super::support::assert_deterministic;

#[test]
fn memory_tie_break_is_canonicalized() {
    assert_deterministic("break-memory-tie");
}
