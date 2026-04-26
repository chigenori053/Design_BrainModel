use super::support::assert_break;

#[test]
fn detects_unsnapshotted_knowledge_order_break() {
    assert_break(
        "break-knowledge-order",
        "knowledge",
        "ExternalNondeterminism",
    );
}
