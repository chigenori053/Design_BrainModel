use super::support::assert_break;

#[test]
fn detects_websearch_nondeterminism_break() {
    assert_break(
        "break-websearch-nondet",
        "knowledge",
        "ExternalNondeterminism",
    );
}
