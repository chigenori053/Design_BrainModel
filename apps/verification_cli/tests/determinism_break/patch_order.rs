use super::support::assert_break;

#[test]
fn detects_patch_order_break() {
    assert_break("break-patch-order", "patch", "PatchBug");
}
