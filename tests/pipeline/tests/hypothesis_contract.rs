use std::collections::{BTreeMap, BTreeSet};

use design_search_engine::stable_v03::{
    Constraint, DeterministicBeamSearchEngine, RecallContext, RecalledPattern,
};
use architecture_ir::stable_v03::{ArchitectureGraphBuilder, Node, NodeType};
use pipeline_tests::{extract_fn_body, read_workspace_file};
use world_model::stable_v03::IntentState;

fn recall_context() -> RecallContext {
    let architecture = ArchitectureGraphBuilder::new()
        .add_node(Node::new("api", NodeType::Interface))
        .add_node(Node::new("service", NodeType::Service))
        .build()
        .expect("valid graph");
    RecallContext {
        patterns: vec![RecalledPattern {
            record_id: "seed".to_string(),
            architecture,
            score: 0.9,
            tags: vec!["api".to_string(), "service".to_string()],
        }],
        constraints: vec![Constraint {
            key: "contains".to_string(),
            value: "service".to_string(),
        }],
        confidence: 0.9,
    }
}

#[test]
fn hypotheses_form_a_dag_with_unique_state_hashes() {
    let engine = DeterministicBeamSearchEngine::default();
    let input = engine.contract_input(
        &IntentState {
            raw: "api service cache".to_string(),
            tokens: vec!["api".to_string(), "service".to_string(), "cache".to_string()],
        },
        Some(&recall_context()),
    );
    let snapshot = engine.inspect_hypotheses(input);
    let ids = snapshot
        .hypotheses
        .iter()
        .map(|hypothesis| hypothesis.id)
        .collect::<BTreeSet<_>>();
    let by_id = snapshot
        .hypotheses
        .iter()
        .map(|hypothesis| (hypothesis.id, hypothesis))
        .collect::<BTreeMap<_, _>>();

    assert_eq!(
        snapshot
            .hypotheses
            .iter()
            .map(|hypothesis| hypothesis.state_hash.clone())
            .collect::<BTreeSet<_>>()
            .len(),
        snapshot.hypotheses.len()
    );
    assert!(snapshot
        .hypotheses
        .windows(2)
        .all(|pair| pair[0].depth <= pair[1].depth));
    for hypothesis in &snapshot.hypotheses {
        let mut current = hypothesis.parent;
        let mut seen = BTreeSet::new();
        while let Some(parent) = current {
            assert!(ids.contains(&parent));
            assert!(seen.insert(parent));
            current = by_id.get(&parent).and_then(|parent_hypothesis| parent_hypothesis.parent);
        }
    }
}

#[test]
fn search_layer_does_not_embed_validation_or_decision_in_expansion() {
    let source = read_workspace_file("crates/engine/design_search_engine/src/stable_v03.rs");
    let body = extract_fn_body(&source, "fn expand_hypothesis(");

    assert!(!body.contains("Validation"));
    assert!(!body.contains("Decision"));
}
