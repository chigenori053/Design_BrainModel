use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use design_domain::Layer;
use world_model_core::WorldState;

use crate::experience_store::DesignExperience;
use crate::pattern_store::{DesignPattern, PatternId};

pub fn extract_pattern(pattern_id: PatternId, exp: &DesignExperience) -> DesignPattern {
    DesignPattern {
        pattern_id,
        causal_graph: exp.causal_graph.clone(),
        dependency_edges: exp.dependency_edges.clone(),
        layer_sequence: exp.layer_sequence.clone(),
        frequency: 1,
        average_score: exp.score,
    }
}

pub fn layer_sequence_from_state(state: &WorldState) -> Vec<Layer> {
    let mut layers = state
        .architecture
        .design_units_by_id()
        .values()
        .map(|unit| unit.layer)
        .collect::<Vec<_>>();
    layers.sort_by_key(|layer| layer.order());
    layers
}

pub fn architecture_hash(state: &WorldState) -> u64 {
    let mut hasher = DefaultHasher::new();
    state.state_id.hash(&mut hasher);
    state.depth.hash(&mut hasher);
    for layer in layer_sequence_from_state(state) {
        layer.hash(&mut hasher);
    }
    for edge in &state.architecture.graph.edges {
        edge.hash(&mut hasher);
    }
    for edge in state.architecture.causal_graph().edges() {
        edge.hash(&mut hasher);
    }
    hasher.finish()
}
