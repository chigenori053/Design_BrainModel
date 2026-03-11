use knowledge_engine::KnowledgeConfidence;

#[test]
fn effective_confidence_uses_square_root_revision() {
    let confidence = KnowledgeConfidence::new(0.81, 0.64);

    assert!((confidence.effective_confidence - 0.72).abs() < 1e-9);
}
