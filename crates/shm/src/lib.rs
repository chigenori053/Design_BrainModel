use memory_space::{DesignState, Uuid};

pub type RuleId = Uuid;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuleCategory {
    Structural,
    Performance,
    Reliability,
    Cost,
    Refactor,
    ConstraintPropagation,
}

pub type Precondition = fn(&DesignState) -> bool;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Transformation {
    AddNode,
    RemoveNode,
    ModifyAttribute,
    AddConstraint,
    RewireDependency,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EffectVector {
    pub delta_struct: f64,
    pub delta_field: f64,
    pub delta_risk: f64,
    pub delta_cost: f64,
}

#[derive(Clone, Debug)]
pub struct DesignRule {
    pub id: RuleId,
    pub category: RuleCategory,
    pub priority: f64,
    pub precondition: Precondition,
    pub transformation: Transformation,
    pub expected_effect: EffectVector,
}

#[derive(Clone, Debug, Default)]
pub struct Shm {
    pub rules: Vec<DesignRule>,
}

impl Shm {
    pub fn new(rules: Vec<DesignRule>) -> Self {
        Self { rules }
    }

    pub fn with_default_rules() -> Self {
        Self {
            rules: default_rules(),
        }
    }

    pub fn applicable_rules(&self, state: &DesignState) -> Vec<&DesignRule> {
        self.rules
            .iter()
            .filter(|rule| (rule.precondition)(state))
            .collect()
    }
}

fn default_rules() -> Vec<DesignRule> {
    vec![
        make_rule(
            1001,
            RuleCategory::Refactor,
            0.95,
            precondition_multi_node,
            Transformation::ModifyAttribute,
            effect(0.8, 0.0, -0.2, 0.1),
        ), // Single Responsibility
        make_rule(
            1002,
            RuleCategory::Structural,
            0.90,
            precondition_has_edges,
            Transformation::RewireDependency,
            effect(0.7, 0.0, -0.3, 0.0),
        ), // Reduce Coupling
        make_rule(
            1003,
            RuleCategory::Structural,
            0.86,
            precondition_depth_at_least_two,
            Transformation::AddNode,
            effect(0.6, 0.0, -0.1, 0.1),
        ), // Introduce Layer
        make_rule(
            1004,
            RuleCategory::Performance,
            0.84,
            precondition_multi_node,
            Transformation::AddConstraint,
            effect(0.4, 0.5, 0.0, 0.2),
        ), // Introduce Caching (abstract)
        make_rule(
            1005,
            RuleCategory::Reliability,
            0.82,
            precondition_has_leaf_node,
            Transformation::AddNode,
            effect(0.3, 0.0, -0.6, 0.3),
        ), // Add Redundancy
        make_rule(
            1006,
            RuleCategory::Refactor,
            0.88,
            precondition_large_node,
            Transformation::AddNode,
            effect(0.5, 0.0, -0.2, 0.1),
        ), // Split Node
        make_rule(
            1007,
            RuleCategory::Refactor,
            0.70,
            precondition_multi_node,
            Transformation::RemoveNode,
            effect(0.4, 0.0, 0.1, -0.4),
        ), // Merge Node
        make_rule(
            1008,
            RuleCategory::ConstraintPropagation,
            0.78,
            precondition_depth_over_three,
            Transformation::AddConstraint,
            effect(0.6, 0.0, -0.2, 0.0),
        ), // Limit Depth
        make_rule(
            1009,
            RuleCategory::Structural,
            0.92,
            precondition_high_edge_density,
            Transformation::RewireDependency,
            effect(0.7, 0.0, -0.4, 0.0),
        ), // Remove Cycle (preventive in DAG model)
        make_rule(
            1010,
            RuleCategory::ConstraintPropagation,
            0.80,
            precondition_multi_node,
            Transformation::AddConstraint,
            effect(0.5, 0.0, -0.2, 0.0),
        ), // Add Constraint
        make_rule(
            1011,
            RuleCategory::Refactor,
            0.85,
            precondition_high_edge_density,
            Transformation::ModifyAttribute,
            effect(0.7, 0.0, -0.2, 0.0),
        ), // Reduce Complexity
        make_rule(
            1012,
            RuleCategory::Structural,
            0.83,
            precondition_multi_node,
            Transformation::AddNode,
            effect(0.5, 0.0, -0.2, 0.1),
        ), // Introduce Interface
        make_rule(
            1013,
            RuleCategory::Reliability,
            0.79,
            precondition_has_edges,
            Transformation::AddConstraint,
            effect(0.2, 0.0, -0.5, 0.1),
        ), // Introduce Timeout
        make_rule(
            1014,
            RuleCategory::Reliability,
            0.87,
            precondition_has_leaf_node,
            Transformation::AddConstraint,
            effect(0.3, 0.0, -0.6, 0.2),
        ), // Fail Safe
        make_rule(
            1015,
            RuleCategory::Refactor,
            0.81,
            precondition_multi_node,
            Transformation::ModifyAttribute,
            effect(0.6, 0.0, -0.3, 0.0),
        ), // Partition Responsibility
        make_rule(
            1016,
            RuleCategory::Structural,
            0.77,
            precondition_has_edges,
            Transformation::RewireDependency,
            effect(0.6, 0.0, -0.2, 0.0),
        ), // Abstract Dependency
        make_rule(
            1017,
            RuleCategory::Cost,
            0.76,
            precondition_resource_heavy,
            Transformation::AddConstraint,
            effect(0.1, 0.0, 0.0, -0.7),
        ), // Resource Cap
        make_rule(
            1018,
            RuleCategory::Structural,
            0.75,
            precondition_high_fanout,
            Transformation::RewireDependency,
            effect(0.6, 0.0, -0.2, -0.1),
        ), // Minimize Dependency Fanout
        make_rule(
            1019,
            RuleCategory::Refactor,
            0.74,
            precondition_multi_node,
            Transformation::RemoveNode,
            effect(0.5, 0.0, -0.1, -0.2),
        ), // Consolidate Nodes
        make_rule(
            1020,
            RuleCategory::Refactor,
            0.89,
            precondition_large_node,
            Transformation::ModifyAttribute,
            effect(0.8, 0.0, -0.2, -0.1),
        ), // Simplify Structure
    ]
}

fn make_rule(
    id: u128,
    category: RuleCategory,
    priority: f64,
    precondition: Precondition,
    transformation: Transformation,
    expected_effect: EffectVector,
) -> DesignRule {
    DesignRule {
        id: RuleId::from_u128(id),
        category,
        priority,
        precondition,
        transformation,
        expected_effect,
    }
}

fn effect(delta_struct: f64, delta_field: f64, delta_risk: f64, delta_cost: f64) -> EffectVector {
    EffectVector {
        delta_struct,
        delta_field,
        delta_risk,
        delta_cost,
    }
}

fn node_count(state: &DesignState) -> usize {
    state.graph.nodes().len()
}

fn edge_count(state: &DesignState) -> usize {
    state.graph.edges().len()
}

fn precondition_multi_node(state: &DesignState) -> bool {
    node_count(state) >= 2
}

fn precondition_has_edges(state: &DesignState) -> bool {
    edge_count(state) > 0
}

fn precondition_has_leaf_node(state: &DesignState) -> bool {
    let mut outgoing = std::collections::BTreeMap::new();
    for (from, _) in state.graph.edges() {
        *outgoing.entry(*from).or_insert(0usize) += 1;
    }

    state
        .graph
        .nodes()
        .keys()
        .any(|id| !outgoing.contains_key(id))
}

fn precondition_large_node(state: &DesignState) -> bool {
    state
        .graph
        .nodes()
        .values()
        .any(|node| node.attributes.len() >= 3)
}

fn precondition_depth_at_least_two(state: &DesignState) -> bool {
    edge_count(state) >= 2
}

fn precondition_depth_over_three(state: &DesignState) -> bool {
    edge_count(state) >= 4
}

fn precondition_high_edge_density(state: &DesignState) -> bool {
    let n = node_count(state);
    if n <= 1 {
        return false;
    }
    edge_count(state) > n
}

fn precondition_resource_heavy(state: &DesignState) -> bool {
    node_count(state) >= 4
}

fn precondition_high_fanout(state: &DesignState) -> bool {
    let mut outgoing = std::collections::BTreeMap::new();
    for (from, _) in state.graph.edges() {
        *outgoing.entry(*from).or_insert(0usize) += 1;
    }
    outgoing.values().any(|count| *count >= 2)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use memory_space::{DesignNode, DesignState, StructuralGraph, Uuid, Value};

    use crate::{RuleId, Shm};

    fn state_with_graph(node_specs: &[(u128, usize)], edges: &[(u128, u128)]) -> DesignState {
        let mut graph = StructuralGraph::default();

        for (id, attr_count) in node_specs {
            let mut attrs = BTreeMap::new();
            for idx in 0..*attr_count {
                attrs.insert(format!("a{idx}"), Value::Int(idx as i64));
            }
            let node = DesignNode::new(Uuid::from_u128(*id), format!("N{id}"), attrs);
            graph = graph.with_node_added(node);
        }

        for (from, to) in edges {
            graph = graph.with_edge_added(Uuid::from_u128(*from), Uuid::from_u128(*to));
        }

        DesignState::new(Uuid::from_u128(9000), Arc::new(graph), "snapshot")
    }

    #[test]
    fn precondition_true_false_validation() {
        let shm = Shm::with_default_rules();

        let simple = state_with_graph(&[(1, 0)], &[]);
        let connected = state_with_graph(&[(1, 0), (2, 0)], &[(1, 2)]);

        let simple_ids: Vec<RuleId> = shm
            .applicable_rules(&simple)
            .iter()
            .map(|rule| rule.id)
            .collect();
        let connected_ids: Vec<RuleId> = shm
            .applicable_rules(&connected)
            .iter()
            .map(|rule| rule.id)
            .collect();

        assert!(!simple_ids.contains(&RuleId::from_u128(1002))); // needs edges
        assert!(connected_ids.contains(&RuleId::from_u128(1002))); // has edges
    }

    #[test]
    fn rule_filtering_correctness() {
        let shm = Shm::with_default_rules();
        let connected = state_with_graph(&[(1, 0), (2, 0)], &[(1, 2)]);

        let applicable = shm.applicable_rules(&connected);
        assert!(!applicable.is_empty());
        assert!(
            applicable
                .iter()
                .all(|rule| (rule.precondition)(&connected))
        );
        assert!(shm.rules.len() >= 20);
    }

    #[test]
    fn deterministic_output() {
        let shm = Shm::with_default_rules();
        let state = state_with_graph(&[(1, 3), (2, 0), (3, 0)], &[(1, 2), (1, 3)]);

        let first: Vec<RuleId> = shm
            .applicable_rules(&state)
            .iter()
            .map(|rule| rule.id)
            .collect();
        let second: Vec<RuleId> = shm
            .applicable_rules(&state)
            .iter()
            .map(|rule| rule.id)
            .collect();

        assert_eq!(first, second);
    }
}
