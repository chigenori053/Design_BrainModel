use std::collections::BTreeMap;
use std::sync::Arc;

use hybrid_vm::{DesignRule, EffectVector, RuleCategory, RuleId, Transformation};
use memory_space::{DesignNode, DesignState, StateId, StructuralGraph, Uuid, Value};

use crate::MacroOperator;

pub fn apply_atomic(rule: &DesignRule, state: &DesignState) -> DesignState {
    let graph = &state.graph;
    let next_graph = match rule.transformation {
        Transformation::AddNode => apply_add_node(graph, rule),
        Transformation::RemoveNode => apply_remove_node(graph),
        Transformation::ModifyAttribute => apply_modify_attribute(graph, rule),
        Transformation::AddConstraint => apply_add_constraint(graph, rule),
        Transformation::RewireDependency => apply_rewire_dependency(graph),
    };

    let next_snapshot = append_rule_history(&state.profile_snapshot, rule.id);
    let next_id = deterministic_state_id(
        state,
        rule,
        &next_snapshot,
        next_graph.nodes().len(),
        next_graph.edges().len(),
    );

    DesignState::new(next_id, Arc::new(next_graph), next_snapshot)
}

pub fn apply_macro(op: &MacroOperator, state: &DesignState) -> DesignState {
    let mut current = state.clone();
    for (idx, step) in op.steps.iter().take(op.max_activations).enumerate() {
        let rule = DesignRule {
            id: deterministic_uuid(op.id.as_u128(), idx as u128 + 1, 0xAA),
            category: RuleCategory::Refactor,
            priority: 0.5,
            precondition: |_| true,
            transformation: step.clone(),
            expected_effect: EffectVector {
                delta_struct: 0.0,
                delta_field: 0.0,
                delta_risk: 0.0,
                delta_cost: 0.0,
            },
        };
        current = apply_atomic(&rule, &current);
    }
    current
}

fn apply_add_node(graph: &StructuralGraph, rule: &DesignRule) -> StructuralGraph {
    let mut next = graph.clone();
    let node_id = deterministic_uuid(rule.id.as_u128(), graph.nodes().len() as u128 + 1, 0xA1);

    let mut attrs = BTreeMap::new();
    attrs.insert(
        format!("generated_by_{}", rule.id.as_u128()),
        Value::Bool(true),
    );
    let node = DesignNode::new(node_id, "GeneratedNode", attrs);
    next = next.with_node_added(node);

    if let Some(existing_id) = sorted_node_ids(graph).first().copied() {
        next = next.with_edge_added(existing_id, node_id);
    }
    next
}

fn apply_remove_node(graph: &StructuralGraph) -> StructuralGraph {
    let mut ids = sorted_node_ids(graph);
    if let Some(last) = ids.pop() {
        graph.with_node_removed(last)
    } else {
        graph.clone()
    }
}

fn apply_modify_attribute(graph: &StructuralGraph, rule: &DesignRule) -> StructuralGraph {
    let _ = rule;
    graph.clone()
}

fn apply_add_constraint(graph: &StructuralGraph, rule: &DesignRule) -> StructuralGraph {
    let ids = sorted_node_ids(graph);
    if ids.len() >= 2 {
        let from = ids[0];
        let to = ids[1];
        if graph.edges().iter().any(|edge| edge.0 == from && edge.1 == to) {
            graph.clone()
        } else {
            let _ = rule;
            graph.with_edge_added(from, to)
        }
    } else {
        graph.clone()
    }
}

fn apply_rewire_dependency(graph: &StructuralGraph) -> StructuralGraph {
    let ids = sorted_node_ids(graph);
    if ids.len() >= 3 {
        graph.with_edge_added(ids[2], ids[0])
    } else {
        graph.clone()
    }
}

fn sorted_node_ids(graph: &StructuralGraph) -> Vec<Uuid> {
    let mut ids = graph.nodes().keys().copied().collect::<Vec<_>>();
    ids.sort();
    ids
}

fn append_rule_history(snapshot: &str, rule_id: RuleId) -> String {
    let mut history = parse_rule_history(snapshot);
    history.push(rule_id);
    let serialized = history
        .iter()
        .map(|id| id.as_u128().to_string())
        .collect::<Vec<_>>()
        .join(",");
    format!("history:{serialized}")
}

fn parse_rule_history(snapshot: &str) -> Vec<RuleId> {
    snapshot
        .strip_prefix("history:")
        .unwrap_or("")
        .split(',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse::<u128>().ok().map(Uuid::from_u128))
        .collect()
}

fn deterministic_state_id(
    state: &DesignState,
    rule: &DesignRule,
    snapshot: &str,
    node_count: usize,
    edge_count: usize,
) -> StateId {
    let mut acc = 0xcbf29ce484222325u128;
    acc = fnv_mix_u128(acc, state.id.as_u128());
    acc = fnv_mix_u128(acc, rule.id.as_u128());
    acc = fnv_mix_u128(acc, node_count as u128);
    acc = fnv_mix_u128(acc, edge_count as u128);
    for b in snapshot.as_bytes() {
        acc = fnv_mix_u128(acc, *b as u128);
    }
    Uuid::from_u128(acc)
}

fn deterministic_uuid(a: u128, b: u128, salt: u128) -> Uuid {
    let mut acc = 0x9e3779b97f4a7c15u128;
    acc = fnv_mix_u128(acc, a);
    acc = fnv_mix_u128(acc, b);
    acc = fnv_mix_u128(acc, salt);
    Uuid::from_u128(acc)
}

fn fnv_mix_u128(acc: u128, value: u128) -> u128 {
    let prime = 0x100000001b3u128;
    (acc ^ value).wrapping_mul(prime)
}
