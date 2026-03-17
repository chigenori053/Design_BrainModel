use std::collections::BTreeSet;

use architecture_ir::stable_v03::{
    ArchitectureGraph, ArchitectureGraphBuilder, Edge, Node, NodeType, RelationType,
};
use world_model::stable_v03::{ArchitectureState as WorldArchitectureState, IntentState};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constraint {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecalledPattern {
    pub record_id: String,
    pub architecture: ArchitectureGraph,
    pub score: f64,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecallContext {
    pub patterns: Vec<RecalledPattern>,
    pub constraints: Vec<Constraint>,
    pub confidence: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchInput {
    pub intent: IntentState,
    pub recall: Option<RecallContext>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArchitectureCandidate {
    pub id: String,
    pub architecture: ArchitectureGraph,
    pub score: f64,
    pub depth: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchState {
    pub architecture: ArchitectureGraph,
    pub base_score: f64,
    pub score: f64,
    pub depth: usize,
}

pub trait DesignSearchEngine: Send + Sync {
    fn search(&self, input: SearchInput) -> Vec<ArchitectureCandidate>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeterministicBeamSearchEngine {
    pub beam_width: usize,
    pub max_depth: usize,
}

impl Default for DeterministicBeamSearchEngine {
    fn default() -> Self {
        Self {
            beam_width: 3,
            max_depth: 2,
        }
    }
}

impl DesignSearchEngine for DeterministicBeamSearchEngine {
    fn search(&self, input: SearchInput) -> Vec<ArchitectureCandidate> {
        let mut beam = seed_states(&input);
        let mut all_states = beam.clone();
        let active_beam_width = effective_beam_width(self.beam_width, input.recall.as_ref());

        for depth in 0..self.max_depth {
            let mut expanded = beam
                .iter()
                .flat_map(|candidate| expand_state(candidate, &input, depth + 1))
                .collect::<Vec<_>>();
            rank_states(&mut expanded, &input);
            expanded.truncate(active_beam_width.max(1));
            if expanded.is_empty() {
                break;
            }
            all_states.extend(expanded.clone());
            beam = expanded;
        }

        rank_states(&mut all_states, &input);
        all_states
            .into_iter()
            .map(|search_state| ArchitectureCandidate {
                id: candidate_id(&search_state.architecture, search_state.depth),
                architecture: search_state.architecture,
                score: search_state.score,
                depth: search_state.depth,
            })
            .collect()
    }
}

fn seed_states(input: &SearchInput) -> Vec<SearchState> {
    let mut seeds = Vec::new();
    if let Some(recall) = &input.recall {
        for pattern in &recall.patterns {
            seeds.push(SearchState {
                architecture: pattern.architecture.clone(),
                base_score: pattern.score,
                score: pattern.score,
                depth: 0,
            });
        }
    }
    if seeds.is_empty() {
        seeds.push(SearchState {
            architecture: ArchitectureGraph::default(),
            base_score: 0.0,
            score: 0.0,
            depth: 0,
        });
    }
    dedup_states(seeds)
}

fn expand_state(base: &SearchState, input: &SearchInput, depth: usize) -> Vec<SearchState> {
    let tokens = candidate_tokens(input);
    let existing = base
        .architecture
        .nodes()
        .iter()
        .map(|node| node.id.0.clone())
        .collect::<BTreeSet<_>>();

    let mut expansions = Vec::new();
    for token in tokens {
        if existing.contains(&token) {
            continue;
        }
        let node = Node::new(token.clone(), classify_token(&token));
        let graph = if base.architecture.nodes().is_empty() {
            ArchitectureGraphBuilder::new()
                .add_node(node)
                .build()
                .unwrap_or_default()
        } else {
            let predecessor = base
                .architecture
                .nodes()
                .last()
                .map(|existing| existing.id.clone())
                .expect("base architecture has nodes");
            let mut builder = base
                .architecture
                .nodes()
                .iter()
                .cloned()
                .fold(ArchitectureGraphBuilder::new(), |builder, node| {
                    builder.add_node(node)
                });
            builder = base
                .architecture
                .edges()
                .iter()
                .cloned()
                .fold(builder, |builder, edge| builder.add_edge(edge));
            builder
                .add_node(node.clone())
                .add_edge(Edge::new(
                    predecessor,
                    node.id.clone(),
                    RelationType::DependsOn,
                ))
                .build()
                .unwrap_or_default()
        };
        expansions.push(SearchState {
            architecture: graph,
            base_score: 0.0,
            score: 0.0,
            depth,
        });
    }
    dedup_states(expansions)
}

fn candidate_tokens(input: &SearchInput) -> Vec<String> {
    let mut tokens = BTreeSet::new();
    if let Some(recall) = &input.recall {
        for pattern in &recall.patterns {
            for node in pattern.architecture.nodes() {
                tokens.insert(node.id.0.clone());
            }
            for tag in &pattern.tags {
                tokens.insert(tag.clone());
            }
        }
    }
    if tokens.is_empty() {
        for token in &input.intent.tokens {
            tokens.insert(token.clone());
        }
    }
    tokens.into_iter().collect()
}

fn rank_states(states: &mut Vec<SearchState>, input: &SearchInput) {
    for state in states.iter_mut() {
        state.base_score = base_score(state, input);
        let recall_weight = recall_weight(state, input.recall.as_ref());
        state.score = state.base_score * (1.0 + recall_weight);
    }
    states.sort_by(|lhs, rhs| {
        rhs.score.total_cmp(&lhs.score).then_with(|| {
            candidate_id(&lhs.architecture, lhs.depth)
                .cmp(&candidate_id(&rhs.architecture, rhs.depth))
        })
    });
    *states = dedup_states(std::mem::take(states));
}

fn dedup_states(states: Vec<SearchState>) -> Vec<SearchState> {
    let mut seen = BTreeSet::new();
    let mut deduped = Vec::new();
    for state in states {
        let key = candidate_id(&state.architecture, state.depth);
        if seen.insert(key) {
            deduped.push(state);
        }
    }
    deduped
}

fn base_score(state: &SearchState, input: &SearchInput) -> f64 {
    let node_overlap = state
        .architecture
        .nodes()
        .iter()
        .filter(|node| input.intent.tokens.iter().any(|token| token == &node.id.0))
        .count() as f64;
    let structural_bonus = state.architecture.edges().len() as f64 * 0.25
        + state.architecture.nodes().len() as f64 * 0.1;
    let depth_penalty = state.depth as f64 * 0.05;
    node_overlap + structural_bonus - depth_penalty
}

fn recall_weight(state: &SearchState, recall: Option<&RecallContext>) -> f64 {
    let Some(recall) = recall else {
        return 0.0;
    };
    if recall.patterns.is_empty() {
        return 0.0;
    }
    let pattern_match = recall
        .patterns
        .iter()
        .filter(|pattern| {
            pattern
                .architecture
                .nodes()
                .iter()
                .any(|node| state.architecture.node(&node.id).is_some())
        })
        .count() as f64
        / recall.patterns.len() as f64;
    let constraint_bonus = recall.constraints.len() as f64 * 0.05;
    (recall.confidence * pattern_match + constraint_bonus).clamp(0.0, 1.0)
}

fn classify_token(token: &str) -> NodeType {
    if token.contains("api") || token.contains("service") {
        NodeType::Service
    } else if token.contains("db") || token.contains("store") {
        NodeType::DataStore
    } else if token.contains("ui") || token.contains("web") {
        NodeType::Interface
    } else {
        NodeType::Component
    }
}

fn effective_beam_width(base_width: usize, recall: Option<&RecallContext>) -> usize {
    let Some(recall) = recall else {
        return base_width.max(1);
    };
    if recall.patterns.is_empty() {
        return base_width.max(1);
    }
    let reduction = (recall.confidence * base_width as f64).floor() as usize;
    base_width.saturating_sub(reduction).max(1)
}

fn candidate_id(graph: &ArchitectureGraph, depth: usize) -> String {
    let mut parts = graph
        .nodes()
        .iter()
        .map(|node| node.id.0.clone())
        .collect::<Vec<_>>();
    parts.sort();
    format!("depth-{depth}:{}", parts.join("|"))
}

impl From<ArchitectureCandidate> for WorldArchitectureState {
    fn from(value: ArchitectureCandidate) -> Self {
        Self {
            graph: value.architecture,
            candidate_id: Some(value.id),
            score: Some(value.score),
        }
    }
}
