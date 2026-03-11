use knowledge_lifecycle::KnowledgeTurnoverMonitor;

#[test]
fn turnover_metrics_capture_added_and_removed_relations() {
    let metrics = KnowledgeTurnoverMonitor.analyze(10, 9, 2);

    assert_eq!(metrics.added_relations, 0);
    assert_eq!(metrics.removed_relations, 2);
    assert!((metrics.turnover_rate - (2.0 / 19.0)).abs() < 1e-9);
}
