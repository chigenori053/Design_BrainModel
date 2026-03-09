use language_core::{LanguageEvaluator, language_search, semantic_parser};

#[test]
fn language_search_improves_language_score() {
    let initial = semantic_parser("Build a scalable REST API");
    let searched = language_search(initial.clone());
    let evaluator = LanguageEvaluator;

    assert!(evaluator.evaluate(&searched).total() >= evaluator.evaluate(&initial).total());
    assert!(searched.generated_sentence.is_some());
}
