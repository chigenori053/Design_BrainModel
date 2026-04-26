use super::support::assert_break;

#[test]
fn detects_codegen_order_break() {
    assert_break("break-codegen-order", "code", "CodegenBug");
}
