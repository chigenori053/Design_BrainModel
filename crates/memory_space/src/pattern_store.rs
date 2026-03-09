use std::collections::HashMap;

use causal_domain::CausalGraph;
use design_domain::Layer;
use world_model_core::WorldState;

use crate::experience_store::{DesignExperience, ExperienceStore};
use crate::pattern_extractor::{architecture_hash, extract_pattern};
use crate::pattern_matcher::match_patterns;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct PatternId(pub u64);

#[derive(Clone, Debug, PartialEq)]
pub struct DesignPattern {
    pub pattern_id: PatternId,
    pub causal_graph: CausalGraph,
    pub dependency_edges: Vec<(u64, u64)>,
    pub layer_sequence: Vec<Layer>,
    pub frequency: usize,
    pub average_score: f64,
}

pub trait MemorySpace {
    fn recall_patterns(&self, state: &WorldState) -> Vec<DesignPattern>;
    fn store_experience(&mut self, exp: DesignExperience);
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PatternStore {
    pub patterns: Vec<DesignPattern>,
    causal_index: HashMap<String, PatternId>,
    architecture_index: HashMap<u64, PatternId>,
    next_id: u64,
}

impl PatternStore {
    pub fn upsert(&mut self, pattern: DesignPattern, architecture_hash: u64) {
        let signature = signature_for_pattern(&pattern);
        if let Some(pattern_id) = self.causal_index.get(&signature).copied() {
            if let Some(existing) = self
                .patterns
                .iter_mut()
                .find(|candidate| candidate.pattern_id == pattern_id)
            {
                let total =
                    existing.average_score * existing.frequency as f64 + pattern.average_score;
                existing.frequency += pattern.frequency;
                existing.average_score = total / existing.frequency as f64;
                self.architecture_index
                    .insert(architecture_hash, existing.pattern_id);
            }
            return;
        }

        self.causal_index.insert(signature, pattern.pattern_id);
        self.architecture_index
            .insert(architecture_hash, pattern.pattern_id);
        self.patterns.push(pattern);
        self.patterns.sort_by(|lhs, rhs| {
            rhs.average_score
                .total_cmp(&lhs.average_score)
                .then_with(|| rhs.frequency.cmp(&lhs.frequency))
                .then_with(|| lhs.pattern_id.cmp(&rhs.pattern_id))
        });
    }

    pub fn next_pattern_id(&mut self) -> PatternId {
        self.next_id += 1;
        PatternId(self.next_id)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InMemoryMemorySpace {
    pub experience_store: ExperienceStore,
    pub pattern_store: PatternStore,
}

impl InMemoryMemorySpace {
    pub fn with_bootstrap_patterns() -> Self {
        let mut memory = Self::default();
        memory.seed_rest_api_pattern();
        memory
    }

    fn seed_rest_api_pattern(&mut self) {
        let mut graph = CausalGraph::new();
        graph.add_edge(1, 2, causal_domain::CausalRelationKind::Requires);
        graph.add_edge(2, 3, causal_domain::CausalRelationKind::Requires);
        self.store_experience(DesignExperience {
            semantic_context: Default::default(),
            inferred_semantics: Default::default(),
            architecture: default_bootstrap_architecture(),
            architecture_hash: 0xC0DEC0DE,
            causal_graph: graph,
            dependency_edges: vec![(1, 2), (2, 3)],
            layer_sequence: vec![Layer::Ui, Layer::Service, Layer::Repository],
            score: 0.95,
            search_depth: 3,
        });
    }

    pub fn experience_count(&self) -> usize {
        self.experience_store.experiences().len()
    }
}

impl MemorySpace for InMemoryMemorySpace {
    fn recall_patterns(&self, state: &WorldState) -> Vec<DesignPattern> {
        match_patterns(state, &self.pattern_store)
            .into_iter()
            .map(|matched| matched.pattern)
            .collect()
    }

    fn store_experience(&mut self, exp: DesignExperience) {
        if !self.experience_store.update_experience(exp.clone()) {
            return;
        }
        let pattern = extract_pattern(self.pattern_store.next_pattern_id(), &exp);
        self.pattern_store.upsert(pattern, exp.architecture_hash);
    }
}

pub fn signature_for_pattern(pattern: &DesignPattern) -> String {
    let mut nodes = pattern.causal_graph.nodes().copied().collect::<Vec<_>>();
    nodes.sort_unstable();
    let node_order = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (*node, index))
        .collect::<HashMap<_, _>>();
    let mut edges = pattern
        .causal_graph
        .edges()
        .iter()
        .map(|edge| {
            format!(
                "{}:{}:{:?}",
                node_order[&edge.from], node_order[&edge.to], edge.kind
            )
        })
        .collect::<Vec<_>>();
    edges.sort();
    let layers = pattern
        .layer_sequence
        .iter()
        .map(|layer| layer.as_str().to_string())
        .collect::<Vec<_>>()
        .join(">");
    format!("nodes={nodes:?};edges={edges:?};layers={layers}")
}

pub fn store_state_experience(memory: &mut impl MemorySpace, state: &WorldState, score: f64) {
    memory.store_experience(DesignExperience {
        semantic_context: Default::default(),
        inferred_semantics: Default::default(),
        architecture: state.architecture.clone(),
        architecture_hash: architecture_hash(state),
        causal_graph: state.architecture.causal_graph(),
        dependency_edges: state.architecture.graph.edges.clone(),
        layer_sequence: crate::pattern_extractor::layer_sequence_from_state(state),
        score,
        search_depth: state.depth,
    });
}

fn default_bootstrap_architecture() -> design_domain::Architecture {
    let mut architecture = design_domain::Architecture::seeded();
    architecture.add_design_unit(design_domain::DesignUnit::with_layer(
        1,
        "Controller",
        Layer::Ui,
    ));
    architecture.add_design_unit(design_domain::DesignUnit::with_layer(
        2,
        "Service",
        Layer::Service,
    ));
    architecture.add_design_unit(design_domain::DesignUnit::with_layer(
        3,
        "Repository",
        Layer::Repository,
    ));
    architecture
}
