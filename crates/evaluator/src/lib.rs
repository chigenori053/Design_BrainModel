use field_engine::{resonance_score, FieldEngine, TargetField};
use memory_space::DesignState;

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveVector {
    pub f_struct: f64,
    pub f_field: f64,
    pub f_risk: f64,
    pub f_cost: f64,
}

impl ObjectiveVector {
    pub fn clamped(self) -> Self {
        Self {
            f_struct: clamp01(self.f_struct),
            f_field: clamp01(self.f_field),
            f_risk: clamp01(self.f_risk),
            f_cost: clamp01(self.f_cost),
        }
    }
}

pub trait Evaluator {
    fn evaluate(&self, state: &DesignState) -> ObjectiveVector;
}

#[derive(Clone, Debug)]
pub struct StructuralEvaluator {
    pub max_nodes: usize,
    pub max_edges: usize,
}

impl Default for StructuralEvaluator {
    fn default() -> Self {
        Self {
            max_nodes: 1000,
            max_edges: 5000,
        }
    }
}

impl StructuralEvaluator {
    pub fn new(max_nodes: usize, max_edges: usize) -> Self {
        Self {
            max_nodes,
            max_edges,
        }
    }
}

impl Evaluator for StructuralEvaluator {
    fn evaluate(&self, state: &DesignState) -> ObjectiveVector {
        let graph = &state.graph;
        let nodes = graph.nodes().len();
        let edges = graph.edges().len();

        let node_ratio = ratio(nodes, self.max_nodes);
        let edge_budget_ratio = ratio(edges, self.max_edges);
        let max_possible_edges = nodes.saturating_mul(nodes.saturating_sub(1)) / 2;
        let edge_density = if max_possible_edges == 0 {
            0.0
        } else {
            ratio(edges, max_possible_edges)
        };

        let dag_penalty = if graph.is_dag() { 0.0 } else { 1.0 };
        let normalized_complexity = clamp01(0.45 * node_ratio + 0.45 * edge_density + 0.10 * dag_penalty);

        ObjectiveVector {
            f_struct: 1.0 - normalized_complexity,
            f_field: 0.5,
            f_risk: 0.5,
            f_cost: 1.0 - clamp01(0.6 * node_ratio + 0.4 * edge_budget_ratio),
        }
        .clamped()
    }
}

pub struct FieldAwareEvaluator<'a> {
    pub structural: StructuralEvaluator,
    pub field_engine: &'a FieldEngine,
    pub target_field: &'a TargetField,
}

impl Evaluator for FieldAwareEvaluator<'_> {
    fn evaluate(&self, state: &DesignState) -> ObjectiveVector {
        let mut obj = self.structural.evaluate(state);
        let field = self.field_engine.aggregate_state(state);
        obj.f_field = resonance_score(&field, self.target_field);
        obj.clamped()
    }
}

fn ratio(count: usize, max: usize) -> f64 {
    if max == 0 {
        return 1.0;
    }
    clamp01((count as f64) / (max as f64))
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use field_engine::{FieldEngine, TargetField};
    use memory_space::{DesignNode, DesignState, StructuralGraph, Uuid};

    use crate::{Evaluator, FieldAwareEvaluator, StructuralEvaluator};

    fn state_with_graph(nodes: usize, edges: &[(u128, u128)]) -> DesignState {
        let mut graph = StructuralGraph::default();
        for i in 1..=nodes {
            graph = graph.with_node_added(DesignNode::new(
                Uuid::from_u128(i as u128),
                format!("N{i}"),
                BTreeMap::new(),
            ));
        }
        for (from, to) in edges {
            graph = graph.with_edge_added(Uuid::from_u128(*from), Uuid::from_u128(*to));
        }
        DesignState::new(Uuid::from_u128(99), Arc::new(graph), "history:")
    }

    #[test]
    fn structural_score_calculation_correctness() {
        let evaluator = StructuralEvaluator::new(10, 20);
        let simple = state_with_graph(2, &[]);
        let complex = state_with_graph(8, &[(1, 2), (1, 3), (2, 4), (3, 4), (4, 5), (5, 6), (6, 7), (7, 8)]);

        let simple_obj = evaluator.evaluate(&simple);
        let complex_obj = evaluator.evaluate(&complex);

        assert!(simple_obj.f_struct >= complex_obj.f_struct);
        assert!((0.0..=1.0).contains(&simple_obj.f_struct));
        assert!((0.0..=1.0).contains(&simple_obj.f_cost));
    }

    #[test]
    fn field_score_uses_resonance() {
        let state = state_with_graph(3, &[(1, 2), (2, 3)]);
        let engine = FieldEngine::new(32);
        let target = TargetField::fixed(32);
        let evaluator = FieldAwareEvaluator {
            structural: StructuralEvaluator::default(),
            field_engine: &engine,
            target_field: &target,
        };

        let obj1 = evaluator.evaluate(&state);
        let obj2 = evaluator.evaluate(&state);
        assert_eq!(obj1.f_field, obj2.f_field);
        assert!((0.0..=1.0).contains(&obj1.f_field));
    }
}
