use knowledge_engine::KnowledgeConfidence;

#[test]
fn effective_confidence_is_product_of_inference_and_source_reliability() {
    let confidence = KnowledgeConfidence::new(0.8, 0.75);

    assert!((confidence.effective_confidence - (0.8_f64 * 0.75_f64).sqrt()).abs() < 1e-9);
}
