use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use core_types::ObjectiveVector;
use field_engine::{FieldEngine, TargetField};
use memory_space::{
    DesignState, HolographicVectorStore, InterferenceMode, MemoryInterferenceTelemetry,
    MemorySpace,
};

pub trait Evaluator {
    fn evaluate(&self, state: &DesignState) -> ObjectiveVector;
}

pub struct HybridVM {
    evaluator: StructuralEvaluator,
    memory: MemorySpace,
}

impl HybridVM {
    pub fn new(evaluator: StructuralEvaluator, memory: MemorySpace) -> Self {
        Self { evaluator, memory }
    }

    pub fn with_default_memory(evaluator: StructuralEvaluator) -> Self {
        let path = default_store_path();
        let store = HolographicVectorStore::open(path, 4).expect("failed to initialize store");
        let mode = memory_mode_from_env();
        let lambda = match mode {
            InterferenceMode::Disabled => 0.0,
            InterferenceMode::Contractive => 0.1,
            InterferenceMode::Repulsive => 0.02,
        };
        let memory = MemorySpace::new(store, 0.95, lambda, mode, 256)
            .expect("failed to initialize memory");
        Self::new(evaluator, memory)
    }

    pub fn evaluate(&mut self, state: &DesignState) -> ObjectiveVector {
        let base = self.evaluator.evaluate(state);
        let adjusted = self.memory.apply_interference(&base);
        let depth = infer_depth_from_snapshot(&state.profile_snapshot);
        let _ = self.memory.store(&adjusted, depth);
        adjusted
    }

    pub fn take_memory_telemetry(&mut self) -> MemoryInterferenceTelemetry {
        self.memory.take_telemetry()
    }
}

fn infer_depth_from_snapshot(snapshot: &str) -> usize {
    let Some(raw) = snapshot.strip_prefix("history:") else {
        return 0;
    };
    raw.split(',').filter(|part| !part.is_empty()).count()
}

fn default_store_path() -> PathBuf {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("hybrid_vm_store_{}_{}.bin", std::process::id(), id))
}

fn memory_mode_from_env() -> InterferenceMode {
    let raw = std::env::var("PHASE6_MEMORY_MODE").unwrap_or_else(|_| "v6.1".to_string());
    match raw.to_ascii_lowercase().as_str() {
        "off" | "disabled" | "a" => InterferenceMode::Disabled,
        "v6.0" | "v6_0" | "contractive" | "b" => InterferenceMode::Contractive,
        _ => InterferenceMode::Repulsive,
    }
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
        let max_possible_edges = nodes.saturating_mul(nodes.saturating_sub(1)) / 2;
        let edge_density = if max_possible_edges == 0 {
            0.0
        } else {
            ratio(edges, max_possible_edges)
        };

        let dag_penalty = if graph.is_dag() { 0.0 } else { 1.0 };
        let normalized_complexity =
            clamp01(0.45 * node_ratio + 0.45 * edge_density + 0.10 * dag_penalty);
        let f_field = graph
            .normalized_category_entropy()
            .unwrap_or_else(|| graph.normalized_degree_entropy());
        let f_risk = graph.normalized_degree_variance();
        let f_shape = if nodes < 3 {
            0.0
        } else {
            let clustering = graph.average_clustering_coefficient();
            clamp01(clustering)
        };

        ObjectiveVector {
            f_struct: 1.0 - normalized_complexity,
            f_field,
            f_risk,
            f_shape,
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
        let _ = self.field_engine;
        let _ = self.target_field;
        self.structural.evaluate(state)
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
        let complex = state_with_graph(
            8,
            &[
                (1, 2),
                (1, 3),
                (2, 4),
                (3, 4),
                (4, 5),
                (5, 6),
                (6, 7),
                (7, 8),
            ],
        );

        let simple_obj = evaluator.evaluate(&simple);
        let complex_obj = evaluator.evaluate(&complex);

        assert!(simple_obj.f_struct >= complex_obj.f_struct);
        assert!((0.0..=1.0).contains(&simple_obj.f_struct));
        assert!((0.0..=1.0).contains(&simple_obj.f_shape));
    }

    #[test]
    fn field_score_is_normalized() {
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
        assert_eq!(obj1, obj2);
        assert!((0.0..=1.0).contains(&obj1.f_field));
    }
}
