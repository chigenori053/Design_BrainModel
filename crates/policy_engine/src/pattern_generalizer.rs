use design_domain::{Architecture, Layer};
use memory_space_phase14::{DesignPattern, PatternId};

use crate::policy_model::{AbstractPattern, GraphPattern, Role};

pub fn generalize_pattern(pattern: &DesignPattern) -> AbstractPattern {
    AbstractPattern {
        pattern_id: pattern.pattern_id,
        node_roles: generalize_layers(&pattern.layer_sequence),
        relation_structure: GraphPattern {
            node_count: pattern.causal_graph.nodes().count(),
            dependency_edges: pattern.dependency_edges.len(),
            causal_edges: pattern.causal_graph.edges().len(),
        },
        average_score: quantize(pattern.average_score),
    }
}

pub fn generalize_architecture(architecture: &Architecture) -> AbstractPattern {
    let mut layers = architecture
        .design_units_by_id()
        .values()
        .map(|unit| unit.layer)
        .collect::<Vec<_>>();
    layers.sort_by_key(|layer| layer.order());
    AbstractPattern {
        pattern_id: PatternId(0),
        node_roles: generalize_layers(&layers),
        relation_structure: GraphPattern {
            node_count: architecture.design_unit_count(),
            dependency_edges: architecture.dependencies.len(),
            causal_edges: architecture.causal_graph().edges().len(),
        },
        average_score: 0.0,
    }
}

fn generalize_layers(layers: &[Layer]) -> Vec<Role> {
    layers
        .iter()
        .map(|layer| match layer {
            Layer::Ui => Role::LayerA,
            Layer::Service => Role::LayerB,
            Layer::Repository => Role::LayerC,
            Layer::Database => Role::LayerD,
        })
        .collect()
}

fn quantize(value: f64) -> f64 {
    ((value * 100.0).round() / 100.0).clamp(0.0, 1.0)
}
