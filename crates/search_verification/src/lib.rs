use std::collections::{BTreeMap, BTreeSet};

use design_domain::{Architecture, Dependency, DependencyKind, DesignUnit, DesignUnitId, Layer};
use design_search_engine::{
    BeamSearchController, SearchConfig, SearchController as _, SearchState,
};
use memory_space_phase14::{DesignExperience, MemorySpace, PatternId, architecture_hash};
use policy_engine::{SearchPolicy, generalize_architecture};
use world_model_core::WorldState;

pub fn verification_config(policy_bias: f64) -> SearchConfig {
    SearchConfig {
        max_depth: 10,
        max_candidates: 64,
        beam_width: 32,
        diversity_threshold: 0.85,
        experience_bias: 0.2,
        policy_bias,
    }
}

pub fn rest_api_state() -> WorldState {
    WorldState::from_architecture(
        1,
        build_architecture(&[Layer::Ui, Layer::Service, Layer::Repository]),
        Vec::new(),
    )
}

pub fn layered_state() -> WorldState {
    WorldState::from_architecture(
        2,
        build_architecture(&[
            Layer::Ui,
            Layer::Service,
            Layer::Repository,
            Layer::Database,
        ]),
        Vec::new(),
    )
}

pub fn microservice_state() -> WorldState {
    WorldState::from_architecture(
        3,
        build_architecture(&[
            Layer::Ui,
            Layer::Service,
            Layer::Service,
            Layer::Repository,
            Layer::Database,
        ]),
        Vec::new(),
    )
}

pub fn scenario_states() -> Vec<WorldState> {
    vec![rest_api_state(), layered_state(), microservice_state()]
}

pub fn seed_good_experience(controller: &BeamSearchController, state: &WorldState, score: f64) {
    let mut memory = controller.memory.lock().expect("memory lock");
    memory.store_experience(DesignExperience {
        semantic_context: Default::default(),
        inferred_semantics: Default::default(),
        architecture: state.architecture.clone(),
        architecture_hash: architecture_hash(state),
        causal_graph: state.architecture.causal_graph(),
        dependency_edges: state.architecture.graph.edges.clone(),
        layer_sequence: state
            .architecture
            .design_units_by_id()
            .values()
            .map(|unit| unit.layer)
            .collect(),
        score,
        search_depth: state.depth,
    });
}

pub fn update_policy_from_memory(controller: &BeamSearchController) {
    let experiences = controller
        .memory
        .lock()
        .expect("memory lock")
        .experience_store
        .experiences()
        .to_vec();
    controller
        .policy_store
        .lock()
        .expect("policy lock")
        .update_policy(&experiences);
}

pub fn run_all_scenarios(
    controller: &BeamSearchController,
    config: &SearchConfig,
) -> Vec<SearchState> {
    scenario_states()
        .into_iter()
        .flat_map(|state| controller.search(state, None, config))
        .collect()
}

pub fn unique_architecture_count(states: &[SearchState]) -> usize {
    states
        .iter()
        .map(|state| architecture_hash(&state.world_state))
        .collect::<BTreeSet<_>>()
        .len()
}

pub fn action_entropy(states: &[SearchState]) -> f64 {
    let mut counts = BTreeMap::new();
    for state in states {
        if let Some(action) = &state.source_action {
            *counts.entry(format!("{action:?}")).or_insert(0usize) += 1;
        }
    }
    let total = counts.values().sum::<usize>() as f64;
    if total == 0.0 {
        return 0.0;
    }
    counts
        .values()
        .map(|count| {
            let p = *count as f64 / total;
            -(p * p.ln())
        })
        .sum()
}

pub fn graph_similarity(lhs: &SearchState, rhs: &SearchState) -> f64 {
    let lhs_layers = lhs
        .world_state
        .architecture
        .design_units_by_id()
        .values()
        .map(|unit| unit.layer.as_str().to_string())
        .collect::<Vec<_>>();
    let rhs_layers = rhs
        .world_state
        .architecture
        .design_units_by_id()
        .values()
        .map(|unit| unit.layer.as_str().to_string())
        .collect::<Vec<_>>();
    let lhs_edges = normalized_edges(lhs);
    let rhs_edges = normalized_edges(rhs);
    let layer_slots = lhs_layers.len().max(rhs_layers.len()).max(1);
    let layer_mismatches = (0..layer_slots)
        .filter(|index| lhs_layers.get(*index) != rhs_layers.get(*index))
        .count() as f64;
    let lhs_edge_set = lhs_edges.into_iter().collect::<BTreeSet<_>>();
    let rhs_edge_set = rhs_edges.into_iter().collect::<BTreeSet<_>>();
    let edge_slots = lhs_edge_set.len().max(rhs_edge_set.len()).max(1);
    let edge_mismatches = lhs_edge_set.symmetric_difference(&rhs_edge_set).count() as f64;
    let size_mismatch = lhs
        .world_state
        .architecture
        .design_unit_count()
        .abs_diff(rhs.world_state.architecture.design_unit_count()) as f64;
    let denominator = (layer_slots + edge_slots + 1) as f64;
    let edit_distance = layer_mismatches + edge_mismatches + size_mismatch * 2.0;
    (1.0 - edit_distance / denominator).clamp(0.0, 1.0)
}

pub fn average_similarity(states: &[SearchState]) -> f64 {
    let mut total = 0.0;
    let mut pairs = 0usize;
    for index in 0..states.len() {
        for other in (index + 1)..states.len() {
            total += graph_similarity(&states[index], &states[other]);
            pairs += 1;
        }
    }
    if pairs == 0 {
        0.0
    } else {
        total / pairs as f64
    }
}

pub fn pattern_reuse_rate(states: &[SearchState]) -> f64 {
    let patterns = states
        .iter()
        .map(|state| generalize_architecture(&state.world_state.architecture))
        .collect::<Vec<_>>();
    let unique = patterns
        .iter()
        .map(|pattern| format!("{:?}-{:?}", pattern.node_roles, pattern.relation_structure))
        .collect::<BTreeSet<_>>()
        .len();
    if patterns.is_empty() {
        0.0
    } else {
        1.0 - unique as f64 / patterns.len() as f64
    }
}

pub fn max_pattern_ratio(states: &[SearchState]) -> f64 {
    let mut counts = BTreeMap::new();
    for state in states {
        let pattern = generalize_architecture(&state.world_state.architecture);
        *counts
            .entry(format!(
                "{:?}-{:?}",
                pattern.node_roles, pattern.relation_structure
            ))
            .or_insert(0usize) += 1;
    }
    let total = counts.values().sum::<usize>() as f64;
    if total == 0.0 {
        0.0
    } else {
        counts.values().copied().max().unwrap_or(0) as f64 / total
    }
}

pub fn best_scores_over_iterations(
    controller: &BeamSearchController,
    iterations: usize,
    config: &SearchConfig,
) -> Vec<f64> {
    let mut scores = Vec::with_capacity(iterations);
    let scenarios = scenario_states();
    for index in 0..iterations {
        let state = scenarios[index % scenarios.len()].clone();
        let best = controller
            .search(state, None, config)
            .into_iter()
            .map(|candidate| candidate.score)
            .fold(0.0, f64::max);
        scores.push(best);
    }
    scores
}

pub fn score_variance(scores: &[f64]) -> f64 {
    if scores.is_empty() {
        return 0.0;
    }
    let mean = scores.iter().sum::<f64>() / scores.len() as f64;
    scores
        .iter()
        .map(|score| {
            let delta = score - mean;
            delta * delta
        })
        .sum::<f64>()
        / scores.len() as f64
}

pub fn latest_policy(controller: &BeamSearchController) -> Option<SearchPolicy> {
    controller
        .policy_store
        .lock()
        .expect("policy lock")
        .latest()
        .cloned()
}

pub fn bad_pattern_frequency(states: &[SearchState], bad_layers: &[Layer]) -> f64 {
    if states.is_empty() {
        return 0.0;
    }
    let count = states
        .iter()
        .filter(|state| {
            let layers = state
                .world_state
                .architecture
                .design_units_by_id()
                .values()
                .map(|unit| unit.layer)
                .collect::<Vec<_>>();
            layers == bad_layers
        })
        .count();
    count as f64 / states.len() as f64
}

pub fn pattern_ids_from_policy(controller: &BeamSearchController) -> Vec<PatternId> {
    latest_policy(controller)
        .map(|policy| policy.pattern_weights.keys().copied().collect())
        .unwrap_or_default()
}

fn build_architecture(layers: &[Layer]) -> Architecture {
    let mut architecture = Architecture::seeded();
    architecture.classes[0].structures[0].design_units.clear();
    architecture.dependencies.clear();
    architecture.graph.edges.clear();
    for (index, layer) in layers.iter().copied().enumerate() {
        architecture.add_design_unit(DesignUnit::with_layer(
            index as u64 + 1,
            format!("{layer:?}{index}"),
            layer,
        ));
        if index > 0 {
            let from = index as u64;
            let to = index as u64 + 1;
            architecture.dependencies.push(Dependency {
                from: DesignUnitId(from),
                to: DesignUnitId(to),
                kind: DependencyKind::Calls,
            });
            architecture.graph.edges.push((from, to));
        }
    }
    architecture
}

fn normalized_edges(state: &SearchState) -> Vec<String> {
    let graph = state.world_state.architecture.causal_graph();
    let mut nodes = graph.nodes().copied().collect::<Vec<_>>();
    nodes.sort_unstable();
    let order = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (*node, index))
        .collect::<BTreeMap<_, _>>();
    let mut edges = graph
        .edges()
        .iter()
        .map(|edge| format!("{}:{}:{:?}", order[&edge.from], order[&edge.to], edge.kind))
        .collect::<Vec<_>>();
    edges.sort();
    edges
}
