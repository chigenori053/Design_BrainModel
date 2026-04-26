use super::support::assert_break;

#[test]
fn detects_hashmap_order_break() {
    assert_break("break-hashmap-order", "ir", "IRGenerationBug");
}
