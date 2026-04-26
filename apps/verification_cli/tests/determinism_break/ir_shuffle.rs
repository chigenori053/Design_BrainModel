use super::support::assert_break;

#[test]
fn detects_ir_shuffle_break() {
    assert_break("break-ir-shuffle", "ir", "IRGenerationBug");
}
