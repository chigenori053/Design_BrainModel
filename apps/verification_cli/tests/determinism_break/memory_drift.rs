use super::support::assert_break;

#[test]
fn detects_memory_drift_break() {
    assert_break("break-memory-drift", "memory", "RetrievalInstability");
}
