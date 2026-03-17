use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Node, NodeType};
use design_search_engine::stable_v03::{
    Constraint, DesignSearchEngine, DeterministicBeamSearchEngine, RecallContext, RecalledPattern,
    SearchInput,
};
use world_model::stable_v03::IntentState;

fn test_input() -> SearchInput {
    SearchInput {
        intent: IntentState {
            raw: "api service repository".to_string(),
            tokens: vec![
                "api".to_string(),
                "service".to_string(),
                "repository".to_string(),
            ],
        },
        recall: Some(recall_context()),
    }
}

fn recall_context() -> RecallContext {
    let recalled = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Service))
        .add_node(Node::new("repository", NodeType::Component))
        .build()
        .expect("valid graph");
    RecallContext {
        patterns: vec![RecalledPattern {
            record_id: "memory-1".to_string(),
            architecture: recalled,
            score: 0.9,
            tags: vec!["api".to_string(), "repository".to_string()],
        }],
        constraints: vec![Constraint {
            key: "layer".to_string(),
            value: "service".to_string(),
        }],
        confidence: 0.8,
    }
}

#[test]
fn search_is_pure_for_same_input() {
    let engine = DeterministicBeamSearchEngine::default();
    let input = test_input();

    let first = engine.search(input.clone());
    let second = engine.search(input);

    assert_eq!(first, second);
}

#[test]
fn recall_reduces_search_width() {
    let engine = DeterministicBeamSearchEngine {
        beam_width: 4,
        max_depth: 2,
    };
    let input_without_recall = SearchInput {
        intent: test_input().intent,
        recall: None,
    };
    let input_with_recall = test_input();

    let without_recall = engine.search(input_without_recall);
    let with_recall = engine.search(input_with_recall);

    assert!(with_recall.len() < without_recall.len());
}
