use std::collections::{BTreeMap, HashMap};

use design_domain::Constraint;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldState {
    pub entities: Vec<EntityState>,
    pub environmental_constraints: Vec<Constraint>,
    pub causal_state: CausalState,
    pub world_signature: String,
}

impl WorldState {
    pub fn new(
        entities: Vec<EntityState>,
        environmental_constraints: Vec<Constraint>,
        causal_state: CausalState,
    ) -> Self {
        let mut state = Self {
            entities,
            environmental_constraints,
            causal_state,
            world_signature: String::new(),
        };
        state.refresh_signature();
        state
    }

    pub fn refresh_signature(&mut self) {
        self.entities.sort_by(|left, right| {
            left.entity_id
                .cmp(&right.entity_id)
                .then(left.semantic_role.cmp(&right.semantic_role))
                .then(left.current_state.cmp(&right.current_state))
        });
        self.causal_state.edges.sort_by(CausalEdge::cmp_stable);
        self.environmental_constraints
            .sort_by(|left, right| left.name.cmp(&right.name));
        self.world_signature = stable_world_signature(self);
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EntityState {
    pub entity_id: String,
    pub semantic_role: String,
    pub current_state: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CausalState {
    pub edges: Vec<CausalEdge>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticCausalEngine {
    pub causal_graph: CausalGraph,
    pub consequence_predictor: ConsequencePredictor,
}

impl SemanticCausalEngine {
    pub fn new(causal_graph: CausalGraph) -> Self {
        Self {
            causal_graph,
            consequence_predictor: ConsequencePredictor::default(),
        }
    }

    pub fn predict(
        &self,
        world_state: &WorldState,
        action: &str,
        runtime_identity_hash: impl Into<String>,
        sync: &EnvironmentSync,
    ) -> SemanticRuntimeResult<ConsequenceSimulation> {
        if !sync.is_synchronized_with(world_state) {
            return Err(SemanticRuntimeError::WorldStateStale);
        }

        let mut projected_world_state = world_state.clone();
        projected_world_state.causal_state.edges = self.causal_graph.edges.clone();
        projected_world_state
            .causal_state
            .edges
            .sort_by(CausalEdge::cmp_stable);

        let propagation_path = self
            .consequence_predictor
            .propagate(&mut projected_world_state, action);
        projected_world_state.refresh_signature();

        let causal_stability =
            CausalStability::measure(world_state, &projected_world_state, &self.causal_graph);
        let projected_risk = RiskLevel::from_stability(&causal_stability);
        if projected_risk == RiskLevel::Critical {
            return Err(SemanticRuntimeError::FutureInstabilityOverflow {
                stability: causal_stability,
            });
        }

        Ok(ConsequenceSimulation {
            projected_world_state,
            projected_runtime_state: ProjectionSnapshot {
                semantic_intent: action.to_owned(),
                runtime_identity_hash: runtime_identity_hash.into(),
            },
            projected_risk,
            propagation_path,
            causal_stability,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CausalGraph {
    pub edges: Vec<CausalEdge>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConsequencePredictor;

impl ConsequencePredictor {
    fn propagate(&self, world_state: &mut WorldState, action: &str) -> Vec<String> {
        let mut edges = world_state.causal_state.edges.clone();
        edges.sort_by(CausalEdge::cmp_stable);
        let mut propagation_path = Vec::new();

        for edge in edges {
            if edge.source_state != action {
                continue;
            }

            for entity in &mut world_state.entities {
                if entity.current_state == edge.source_state {
                    entity.current_state = edge.target_state.clone();
                    propagation_path.push(format!(
                        "{}:{}->{}",
                        entity.entity_id, edge.source_state, edge.target_state
                    ));
                }
            }
        }

        propagation_path.sort();
        propagation_path
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CausalEdge {
    pub source_state: String,
    pub target_state: String,
    pub causal_weight: f64,
}

impl CausalEdge {
    fn cmp_stable(left: &Self, right: &Self) -> std::cmp::Ordering {
        left.source_state
            .cmp(&right.source_state)
            .then(left.target_state.cmp(&right.target_state))
            .then(left.causal_weight.total_cmp(&right.causal_weight))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConsequenceSimulation {
    pub projected_world_state: WorldState,
    pub projected_runtime_state: ProjectionSnapshot,
    pub projected_risk: RiskLevel,
    pub propagation_path: Vec<String>,
    pub causal_stability: CausalStability,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectionSnapshot {
    pub semantic_intent: String,
    pub runtime_identity_hash: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    fn from_stability(stability: &CausalStability) -> Self {
        if stability.future_divergence >= 1.0
            || stability.world_consistency < 0.25
            || stability.causal_entropy >= 1.0
        {
            Self::Critical
        } else if stability.future_divergence >= 0.66 || stability.world_consistency < 0.5 {
            Self::High
        } else if stability.future_divergence >= 0.33 || stability.causal_entropy >= 0.5 {
            Self::Medium
        } else {
            Self::Low
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvironmentSync {
    pub sync_state: SyncState,
    pub last_world_update: u64,
    pub synchronized_world_signature: String,
}

impl EnvironmentSync {
    pub fn synchronized(world_state: &WorldState, last_world_update: u64) -> Self {
        Self {
            sync_state: SyncState::Synchronized,
            last_world_update,
            synchronized_world_signature: world_state.world_signature.clone(),
        }
    }

    pub fn stale(last_world_update: u64) -> Self {
        Self {
            sync_state: SyncState::Stale,
            last_world_update,
            synchronized_world_signature: String::new(),
        }
    }

    pub fn is_synchronized_with(&self, world_state: &WorldState) -> bool {
        self.sync_state == SyncState::Synchronized
            && self.synchronized_world_signature == world_state.world_signature
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncState {
    Synchronized,
    Stale,
    Unsynchronized,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticWorldCompression {
    pub compressed_world_groups: Vec<WorldGroup>,
}

impl SemanticWorldCompression {
    pub fn compress(world_state: &WorldState) -> Self {
        let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        for entity in &world_state.entities {
            groups
                .entry((entity.semantic_role.clone(), entity.current_state.clone()))
                .or_default()
                .push(entity.entity_id.clone());
        }

        let compressed_world_groups = groups
            .into_iter()
            .map(|((semantic_role, converged_state), mut entity_ids)| {
                entity_ids.sort();
                WorldGroup {
                    semantic_role,
                    converged_state,
                    entity_ids,
                }
            })
            .collect();

        Self {
            compressed_world_groups,
        }
    }

    pub fn preserves_semantic_causality(&self, world_state: &WorldState) -> bool {
        let compressed_entities: usize = self
            .compressed_world_groups
            .iter()
            .map(|group| group.entity_ids.len())
            .sum();
        compressed_entities == world_state.entities.len()
            && world_state
                .causal_state
                .edges
                .iter()
                .all(|edge| !edge.source_state.is_empty() && !edge.target_state.is_empty())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldGroup {
    pub semantic_role: String,
    pub converged_state: String,
    pub entity_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CausalStability {
    pub causal_entropy: f64,
    pub future_divergence: f64,
    pub world_consistency: f64,
    pub semantic_drift: f64,
}

impl CausalStability {
    pub fn measure(before: &WorldState, after: &WorldState, graph: &CausalGraph) -> Self {
        let causal_entropy = causal_entropy(graph);
        let entity_count = before.entities.len().max(1) as f64;
        let changed_entities = before
            .entities
            .iter()
            .zip(after.entities.iter())
            .filter(|(left, right)| left.current_state != right.current_state)
            .count() as f64;
        let future_divergence = ((changed_entities / entity_count)
            * average_causal_uncertainty(graph)
            + contradiction_ratio(graph))
        .clamp(0.0, 1.0);
        let semantic_drift = (changed_entities / entity_count).clamp(0.0, 1.0);
        let world_consistency = if after
            .causal_state
            .edges
            .iter()
            .all(|edge| !edge.source_state.is_empty() && !edge.target_state.is_empty())
        {
            1.0 - contradiction_ratio(graph)
        } else {
            0.0
        }
        .clamp(0.0, 1.0);

        Self {
            causal_entropy,
            future_divergence,
            world_consistency,
            semantic_drift,
        }
    }

    pub fn requires_halt(&self) -> bool {
        self.causal_entropy >= 1.0
            || self.future_divergence >= 1.0
            || self.world_consistency < 0.25
            || self.semantic_drift >= 1.0
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IdentityPersistence {
    pub persistent_identity_hash: String,
    pub world_lineage: Vec<String>,
}

impl IdentityPersistence {
    pub fn establish(seed: &str, world_state: &WorldState) -> Self {
        Self {
            persistent_identity_hash: stable_hash_hex(seed),
            world_lineage: vec![world_state.world_signature.clone()],
        }
    }

    pub fn transition(&self, world_state: &WorldState) -> Self {
        let mut world_lineage = self.world_lineage.clone();
        world_lineage.push(world_state.world_signature.clone());
        Self {
            persistent_identity_hash: self.persistent_identity_hash.clone(),
            world_lineage,
        }
    }

    pub fn is_continuous_with(&self, other: &Self) -> bool {
        self.persistent_identity_hash == other.persistent_identity_hash
            && other.world_lineage.starts_with(&self.world_lineage)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldAttentionField {
    pub entity_saliency: HashMap<String, f64>,
    pub environmental_focus: f64,
}

impl WorldAttentionField {
    pub fn allocate(world_state: &WorldState, stability: &CausalStability) -> Self {
        let mut entity_saliency = HashMap::new();
        let mut relevance: BTreeMap<String, f64> = BTreeMap::new();
        for edge in &world_state.causal_state.edges {
            *relevance.entry(edge.source_state.clone()).or_default() += edge.causal_weight.abs();
            *relevance.entry(edge.target_state.clone()).or_default() += edge.causal_weight.abs();
        }

        for entity in &world_state.entities {
            let causal_relevance = relevance
                .get(&entity.current_state)
                .copied()
                .unwrap_or_default()
                .min(1.0);
            let saliency =
                (causal_relevance + stability.future_divergence + stability.semantic_drift) / 3.0;
            entity_saliency.insert(entity.entity_id.clone(), saliency.clamp(0.0, 1.0));
        }

        Self {
            entity_saliency,
            environmental_focus: (1.0 - stability.world_consistency + stability.causal_entropy)
                .clamp(0.0, 1.0),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldRuntimeEvaluation {
    pub causal_stability: f64,
    pub semantic_consistency: f64,
    pub future_prediction_accuracy: f64,
}

impl WorldRuntimeEvaluation {
    pub fn evaluate(stability: &CausalStability, prediction_matched: bool) -> Self {
        Self {
            causal_stability: (1.0 - stability.causal_entropy).clamp(0.0, 1.0),
            semantic_consistency: stability.world_consistency,
            future_prediction_accuracy: if prediction_matched { 1.0 } else { 0.0 },
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorldReplayReconstruction {
    pub reconstructed_world_hash: String,
}

impl WorldReplayReconstruction {
    pub fn from_lineage(lineage: &[String]) -> Self {
        let mut canonical = lineage.to_vec();
        canonical.sort();
        Self {
            reconstructed_world_hash: stable_hash_hex(&canonical.join("|")),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FuturePredictionVisibility {
    pub future_consequence_path: Vec<String>,
    pub causal_propagation: Vec<CausalEdge>,
    pub instability_forecast: CausalStability,
    pub world_compression_lineage: Vec<WorldGroup>,
    pub human_approval_required: bool,
}

impl FuturePredictionVisibility {
    pub fn from_simulation(
        simulation: &ConsequenceSimulation,
        compression: &SemanticWorldCompression,
    ) -> Self {
        Self {
            future_consequence_path: simulation.propagation_path.clone(),
            causal_propagation: simulation.projected_world_state.causal_state.edges.clone(),
            instability_forecast: simulation.causal_stability,
            world_compression_lineage: compression.compressed_world_groups.clone(),
            human_approval_required: simulation.projected_risk >= RiskLevel::High,
        }
    }
}

pub type SemanticRuntimeResult<T> = Result<T, SemanticRuntimeError>;

#[derive(Clone, Debug, PartialEq)]
pub enum SemanticRuntimeError {
    WorldStateStale,
    FutureInstabilityOverflow { stability: CausalStability },
}

fn causal_entropy(graph: &CausalGraph) -> f64 {
    if graph.edges.is_empty() {
        return 0.0;
    }

    let contradiction = contradiction_ratio(graph);
    (average_causal_uncertainty(graph) + contradiction).clamp(0.0, 1.0)
}

fn average_causal_uncertainty(graph: &CausalGraph) -> f64 {
    if graph.edges.is_empty() {
        return 0.0;
    }

    let average_weight = graph
        .edges
        .iter()
        .map(|edge| edge.causal_weight.abs().clamp(0.0, 1.0))
        .sum::<f64>()
        / graph.edges.len() as f64;
    (1.0 - average_weight).clamp(0.0, 1.0)
}

fn contradiction_ratio(graph: &CausalGraph) -> f64 {
    let mut targets_by_source: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for edge in &graph.edges {
        targets_by_source
            .entry(edge.source_state.as_str())
            .or_default()
            .push(edge.target_state.as_str());
    }

    let mut contradictory_sources = 0;
    for targets in targets_by_source.values_mut() {
        targets.sort();
        targets.dedup();
        if targets.len() > 1 {
            contradictory_sources += 1;
        }
    }

    if targets_by_source.is_empty() {
        0.0
    } else {
        contradictory_sources as f64 / targets_by_source.len() as f64
    }
}

fn stable_world_signature(world_state: &WorldState) -> String {
    let mut parts = Vec::new();
    for entity in &world_state.entities {
        parts.push(format!(
            "entity:{}:{}:{}",
            entity.entity_id, entity.semantic_role, entity.current_state
        ));
    }
    for constraint in &world_state.environmental_constraints {
        parts.push(format!(
            "constraint:{}:{:?}:{:?}",
            constraint.name, constraint.max_design_units, constraint.max_dependencies
        ));
    }
    for edge in &world_state.causal_state.edges {
        parts.push(format!(
            "edge:{}:{}:{:.12}",
            edge.source_state, edge.target_state, edge.causal_weight
        ));
    }
    stable_hash_hex(&parts.join("|"))
}

fn stable_hash_hex(input: &str) -> String {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}
