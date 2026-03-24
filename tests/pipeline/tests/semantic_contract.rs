use bridge::reasoning_input_from_intent;
use contracts::{Context, Intent, NodeId, Relation, RelationType, SemanticRepresentation};
use pipeline_tests::{extract_fn_body, read_workspace_file};
use world_model::stable_v03::IntentState;

#[test]
fn semantic_hash_and_request_id_are_deterministic() {
    let intent = IntentState {
        raw: "api service db".to_string(),
        tokens: vec!["db".to_string(), "api".to_string(), "service".to_string()],
    };
    let extra = vec!["cache".to_string(), "api".to_string()];
    let context = Context {
        beam_width: 3,
        max_depth: 2,
        timeout_ms: 2_000,
    };

    let lhs = reasoning_input_from_intent(&intent, &extra, context.clone());
    let rhs = reasoning_input_from_intent(&intent, &extra, context);

    assert_eq!(lhs.semantic.hash, rhs.semantic.hash);
    assert_eq!(lhs.request_id, rhs.request_id);
    assert!(
        lhs.semantic
            .intents
            .windows(2)
            .all(|pair| pair[0] <= pair[1])
    );
}

#[test]
fn semantic_representation_normalizes_sorting_and_relations() {
    let semantic = SemanticRepresentation::new(
        vec![
            Relation {
                from: NodeId("b".to_string()),
                to: NodeId("a".to_string()),
                relation_type: RelationType::DependsOn,
            },
            Relation {
                from: NodeId("a".to_string()),
                to: NodeId("a".to_string()),
                relation_type: RelationType::SimilarTo,
            },
        ],
        vec![
            Intent {
                label: "service".to_string(),
            },
            Intent {
                label: "api".to_string(),
            },
            Intent {
                label: "api".to_string(),
            },
        ],
    );

    assert_eq!(
        semantic
            .intents
            .iter()
            .map(|intent| intent.label.as_str())
            .collect::<Vec<_>>(),
        vec!["api", "service"]
    );
    assert_eq!(semantic.relations.len(), 1);
    assert!(semantic.relations[0].from != semantic.relations[0].to);
}

#[test]
fn semantic_layer_does_not_compute_scores_or_access_memory() {
    let source = read_workspace_file("crates/bridge/src/lib.rs");
    let body = extract_fn_body(&source, "pub fn reasoning_input_from_intent(");

    assert!(!body.contains("MemoryCandidate"));
    assert!(!body.contains("score"));
}
