use super::support::assert_deterministic;

#[test]
fn memory_order_break_is_canonicalized() {
    assert_deterministic("break-memory-order");
}
