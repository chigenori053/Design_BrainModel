use super::support::assert_break;

#[test]
fn detects_beam_instability_break() {
    assert_break("break-beam-instability", "search", "SearchOrderingBug");
}
