use std::collections::{BTreeMap, BTreeSet};

use architecture_domain::ArchitectureState;
use knowledge_engine::{
    KnowledgeGraph, KnowledgeRelation, KnowledgeSource, RelationType, ValidationScore,
};
use language_core::SemanticGraph;

#[derive(Clone, Debug, PartialEq)]
pub struct KnowledgeQualityMetrics {
    pub node_count: usize,
    pub edge_count: usize,
    pub conflict_rate: f64,
    pub average_confidence: f64,
}

impl Default for KnowledgeQualityMetrics {
    fn default() -> Self {
        Self {
            node_count: 0,
            edge_count: 0,
            conflict_rate: 0.0,
            average_confidence: 0.0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeHalfLife {
    pub survival_cycles: u64,
    pub half_life: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LifecycleMetrics {
    pub entropy: f64,
    pub average_confidence: f64,
    pub pruning_rate: f64,
    pub reinforcement_rate: f64,
    pub turnover_rate: f64,
    pub half_life: u64,
}

pub type LifecycleStabilityMetrics = LifecycleMetrics;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LifecycleStabilityEngine;

impl LifecycleStabilityEngine {
    pub fn analyze(
        &self,
        graph: &KnowledgeGraph,
        entropy: &KnowledgeEntropy,
        pruned_relations: usize,
        reinforced_relations: usize,
        turnover_metrics: &KnowledgeTurnoverMetrics,
        half_life: u64,
    ) -> LifecycleMetrics {
        LifecycleMetrics {
            entropy: entropy.entropy_score,
            average_confidence: if graph.relations.is_empty() {
                0.0
            } else {
                graph
                    .relations
                    .iter()
                    .map(|relation| relation.confidence.effective_confidence)
                    .sum::<f64>()
                    / graph.relations.len() as f64
            },
            pruning_rate: pruned_relations as f64 / graph.relations.len().max(1) as f64,
            reinforcement_rate: reinforced_relations as f64 / graph.relations.len().max(1) as f64,
            turnover_rate: turnover_metrics.turnover_rate,
            half_life,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeTurnoverMetrics {
    pub added_relations: usize,
    pub removed_relations: usize,
    pub turnover_rate: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KnowledgeLifecycleState {
    pub cycle: u64,
    pub quality_metrics: KnowledgeQualityMetrics,
    pub lifecycle_metrics: LifecycleMetrics,
    pub half_life: KnowledgeHalfLife,
    pub source_reliabilities: Vec<SourceReliability>,
    pub provenance_recorded: usize,
    pub reliability_evaluated: usize,
    pub embeddings_generated: usize,
    pub aged_relations: usize,
    pub reinforced_relations: usize,
    pub pruned_relations: usize,
    pub semantic_clusters: usize,
    pub semantic_pruned_relations: usize,
    pub conflicts_resolved: usize,
    pub diversification_triggered: bool,
    pub exploration_weight: f64,
    pub reinforcement_rate_applied: f64,
    pub turnover_metrics: KnowledgeTurnoverMetrics,
}

impl Default for KnowledgeLifecycleState {
    fn default() -> Self {
        Self {
            cycle: 0,
            quality_metrics: KnowledgeQualityMetrics::default(),
            lifecycle_metrics: LifecycleMetrics::default(),
            half_life: KnowledgeHalfLife::default(),
            source_reliabilities: Vec::new(),
            provenance_recorded: 0,
            reliability_evaluated: 0,
            embeddings_generated: 0,
            aged_relations: 0,
            reinforced_relations: 0,
            pruned_relations: 0,
            semantic_clusters: 0,
            semantic_pruned_relations: 0,
            conflicts_resolved: 0,
            diversification_triggered: false,
            exploration_weight: 1.0,
            reinforcement_rate_applied: 0.02,
            turnover_metrics: KnowledgeTurnoverMetrics::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KnowledgeLifecycleConfig {
    pub decay_rate: f64,
    pub reinforcement_rate: f64,
    pub evaluation_threshold: f64,
    pub frequent_usage_threshold: u64,
    pub max_confidence: f64,
    pub prune_confidence_threshold: f64,
    pub prune_unused_cycles: u64,
    pub entropy_threshold: f64,
    pub similarity_threshold: f32,
    pub current_cycle: u64,
}

impl Default for KnowledgeLifecycleConfig {
    fn default() -> Self {
        Self {
            decay_rate: 0.05,
            reinforcement_rate: 0.02,
            evaluation_threshold: 0.75,
            frequent_usage_threshold: 3,
            max_confidence: 0.9,
            prune_confidence_threshold: 0.2,
            prune_unused_cycles: 5,
            entropy_threshold: 0.6,
            similarity_threshold: 0.85,
            current_cycle: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceReliability {
    pub source: KnowledgeSource,
    pub reliability_score: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeSourceReliabilityEvaluator;

impl KnowledgeSourceReliabilityEvaluator {
    pub fn evaluate(&self, graph: &KnowledgeGraph) -> Vec<SourceReliability> {
        let mut reliabilities = BTreeMap::<String, SourceReliability>::new();
        for relation in &graph.relations {
            let key = format!("{:?}", relation.provenance.source);
            reliabilities
                .entry(key)
                .and_modify(|current| {
                    current.reliability_score = current
                        .reliability_score
                        .max(relation.confidence.source_reliability);
                })
                .or_insert_with(|| SourceReliability {
                    source: relation.provenance.source.clone(),
                    reliability_score: relation.confidence.source_reliability,
                });
        }
        reliabilities.into_values().collect()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReinforcementConfig {
    pub reinforcement_rate: f64,
    pub max_confidence: f64,
}

impl Default for ReinforcementConfig {
    fn default() -> Self {
        Self {
            reinforcement_rate: 0.02,
            max_confidence: 0.9,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeProvenanceTracker {
    pub current_cycle: u64,
}

impl KnowledgeProvenanceTracker {
    pub fn record(&self, knowledge: &mut KnowledgeRelation) {
        if knowledge.provenance.timestamp == 0 {
            knowledge.provenance.timestamp = self.current_cycle;
        }
        knowledge.provenance.usage_count = knowledge.provenance.usage_count.saturating_add(1);
        knowledge.provenance.last_used = self.current_cycle;
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeAgingEngine {
    pub decay_rate: f64,
    pub current_cycle: u64,
}

impl KnowledgeAgingEngine {
    pub fn decay_confidence(&self, knowledge: &mut KnowledgeRelation) {
        let age = self
            .current_cycle
            .saturating_sub(knowledge.provenance.timestamp) as f64;
        let factor = (-self.decay_rate * age).exp();
        knowledge.confidence = knowledge
            .confidence
            .with_inference_confidence(knowledge.confidence.inference_confidence * factor);
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeReinforcementEngine {
    pub config: ReinforcementConfig,
    pub evaluation_threshold: f64,
    pub frequent_usage_threshold: u64,
    pub evaluation_score: f64,
    pub architecture_usage_count: u64,
    pub consistent_inference: bool,
}

impl KnowledgeReinforcementEngine {
    pub fn reinforce(&self, knowledge: &mut KnowledgeRelation) -> bool {
        let should_reinforce = self.evaluation_score >= self.evaluation_threshold
            || self.architecture_usage_count >= self.frequent_usage_threshold
            || self.consistent_inference;
        if !should_reinforce {
            return false;
        }
        let next = (knowledge.confidence.inference_confidence + self.config.reinforcement_rate)
            .min(self.config.max_confidence);
        knowledge.confidence = knowledge.confidence.with_inference_confidence(next);
        true
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeEmbedding {
    pub vector: Vec<f32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticCluster {
    pub relations: Vec<KnowledgeRelation>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SemanticClusterEngine {
    pub similarity_threshold: f32,
}

pub type KnowledgeSemanticCluster = SemanticClusterEngine;

impl SemanticClusterEngine {
    pub fn cluster(&self, graph: &KnowledgeGraph) -> Vec<SemanticCluster> {
        let mut clusters: Vec<Vec<(KnowledgeRelation, KnowledgeEmbedding)>> = Vec::new();
        for relation in &graph.relations {
            let embedding = generate_embedding(relation);
            let mut assigned = false;
            for cluster in &mut clusters {
                let similarity = cosine_similarity(&embedding.vector, &cluster[0].1.vector);
                if cluster[0].0.relation_type == relation.relation_type
                    && similarity >= self.similarity_threshold
                {
                    cluster.push((relation.clone(), embedding.clone()));
                    assigned = true;
                    break;
                }
            }
            if !assigned {
                clusters.push(vec![(relation.clone(), embedding)]);
            }
        }
        clusters
            .into_iter()
            .filter(|cluster| cluster.len() > 1)
            .map(|cluster| SemanticCluster {
                relations: cluster.into_iter().map(|(relation, _)| relation).collect(),
            })
            .collect()
    }

    pub fn prune(&self, graph: &mut KnowledgeGraph) -> usize {
        let clusters = self.cluster(graph);
        if clusters.is_empty() {
            return 0;
        }
        let before = graph.relations.len();
        let mut keep = Vec::new();
        let mut clustered_keys = BTreeSet::new();
        for cluster in clusters {
            if let Some(best) = cluster
                .relations
                .iter()
                .max_by(|lhs, rhs| {
                    lhs.confidence
                        .effective_confidence
                        .total_cmp(&rhs.confidence.effective_confidence)
                })
                .cloned()
            {
                for relation in &cluster.relations {
                    clustered_keys.insert(relation_key(relation));
                }
                keep.push(best);
            }
        }
        let mut survivors = graph
            .relations
            .iter()
            .filter(|relation| !clustered_keys.contains(&relation_key(relation)))
            .cloned()
            .collect::<Vec<_>>();
        survivors.extend(keep);
        survivors.sort_by(|lhs, rhs| relation_key(lhs).cmp(&relation_key(rhs)));
        graph.relations = survivors;
        before.saturating_sub(graph.relations.len())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeEntropy {
    pub entropy_score: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeEntropyMonitor {
    pub entropy_threshold: f64,
}

impl KnowledgeEntropyMonitor {
    pub fn calculate(&self, graph: &KnowledgeGraph) -> KnowledgeEntropy {
        let edge_count = graph.relations.len() as f64;
        if edge_count == 0.0 {
            return KnowledgeEntropy::default();
        }
        let mut counts = BTreeMap::<RelationType, usize>::new();
        for relation in &graph.relations {
            *counts.entry(relation.relation_type).or_insert(0) += 1;
        }
        let entropy_score = counts
            .values()
            .map(|count| {
                let p = *count as f64 / edge_count;
                -(p * p.ln())
            })
            .sum::<f64>();
        KnowledgeEntropy { entropy_score }
    }

    pub fn is_collapse(&self, entropy: &KnowledgeEntropy) -> bool {
        entropy.entropy_score < self.entropy_threshold
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeTurnoverMonitor;

impl KnowledgeTurnoverMonitor {
    pub fn analyze(
        &self,
        previous_relations: usize,
        current_relations: usize,
        removed_relations: usize,
    ) -> KnowledgeTurnoverMetrics {
        let added_relations = current_relations.saturating_sub(previous_relations);
        let total_relations = (previous_relations + current_relations).max(1) as f64;
        KnowledgeTurnoverMetrics {
            added_relations,
            removed_relations,
            turnover_rate: (added_relations + removed_relations) as f64 / total_relations,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeHalfLifeMonitor {
    pub current_cycle: u64,
}

impl KnowledgeHalfLifeMonitor {
    pub fn calculate(&self, graph: &KnowledgeGraph) -> KnowledgeHalfLife {
        let mut survival_cycles = graph
            .relations
            .iter()
            .map(|relation| {
                self.current_cycle
                    .saturating_sub(relation.provenance.timestamp)
            })
            .collect::<Vec<_>>();
        if survival_cycles.is_empty() {
            return KnowledgeHalfLife::default();
        }
        survival_cycles.sort_unstable();
        let mid = survival_cycles.len() / 2;
        let half_life = if survival_cycles.len() % 2 == 0 {
            (survival_cycles[mid - 1] + survival_cycles[mid]) / 2
        } else {
            survival_cycles[mid]
        };
        KnowledgeHalfLife {
            survival_cycles: *survival_cycles.last().unwrap_or(&0),
            half_life,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeConflict {
    pub lhs: KnowledgeRelation,
    pub rhs: KnowledgeRelation,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ConflictContext {
    pub semantic_graph: SemanticGraph,
    pub architecture_context: ArchitectureState,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ResolutionResult {
    pub resolved_relations: Vec<KnowledgeRelation>,
    pub resolved_count: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeConflictResolver;

impl KnowledgeConflictResolver {
    pub fn detect(&self, graph: &KnowledgeGraph) -> Vec<KnowledgeConflict> {
        let mut conflicts = Vec::new();
        for (idx, relation) in graph.relations.iter().enumerate() {
            for other in graph.relations.iter().skip(idx + 1) {
                if relation.source == other.source
                    && relation.target == other.target
                    && relation.relation_type != other.relation_type
                {
                    conflicts.push(KnowledgeConflict {
                        lhs: relation.clone(),
                        rhs: other.clone(),
                    });
                }
            }
        }
        conflicts
    }

    pub fn resolve(
        &self,
        conflicts: Vec<KnowledgeConflict>,
        context: &ConflictContext,
    ) -> ResolutionResult {
        let semantic_labels = context
            .semantic_graph
            .concepts
            .values()
            .map(|concept| concept.label.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        let architecture_roles = context
            .architecture_context
            .components
            .iter()
            .map(|component| format!("{:?}", component.role).to_ascii_lowercase())
            .collect::<BTreeSet<_>>();

        let mut resolved_relations = Vec::new();
        for conflict in conflicts {
            let lhs_score = context_score(&conflict.lhs, &semantic_labels, &architecture_roles);
            let rhs_score = context_score(&conflict.rhs, &semantic_labels, &architecture_roles);
            let winner = if lhs_score >= rhs_score {
                conflict.lhs
            } else {
                conflict.rhs
            };
            resolved_relations.push(winner);
        }
        let resolved_count = resolved_relations.len();
        ResolutionResult {
            resolved_relations,
            resolved_count,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgePruningEngine {
    pub confidence_threshold: f64,
    pub unused_cycles: u64,
    pub current_cycle: u64,
}

impl KnowledgePruningEngine {
    pub fn prune(&self, graph: &mut KnowledgeGraph) -> usize {
        let before = graph.relations.len();
        graph.relations.retain(|relation| {
            let unused_cycles = self
                .current_cycle
                .saturating_sub(relation.provenance.last_used);
            let rejected_conflict = relation.confidence.effective_confidence <= f64::EPSILON;
            relation.confidence.effective_confidence >= self.confidence_threshold
                && unused_cycles < self.unused_cycles
                && !rejected_conflict
        });
        before.saturating_sub(graph.relations.len())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KnowledgeQualityMonitor;

impl KnowledgeQualityMonitor {
    pub fn analyze(&self, graph: &KnowledgeGraph) -> KnowledgeQualityMetrics {
        let edge_count = graph.relations.len();
        let mut conflicts = 0usize;
        for (idx, relation) in graph.relations.iter().enumerate() {
            for other in graph.relations.iter().skip(idx + 1) {
                if relation.source == other.source
                    && relation.target == other.target
                    && relation.relation_type != other.relation_type
                {
                    conflicts += 1;
                }
            }
        }
        let average_confidence = if edge_count == 0 {
            0.0
        } else {
            graph
                .relations
                .iter()
                .map(|relation| relation.confidence.effective_confidence)
                .sum::<f64>()
                / edge_count as f64
        };
        KnowledgeQualityMetrics {
            node_count: graph.entities.len(),
            edge_count,
            conflict_rate: if edge_count == 0 {
                0.0
            } else {
                conflicts as f64 / edge_count as f64
            },
            average_confidence,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct KnowledgeLifecycleEngine {
    pub provenance_tracker: KnowledgeProvenanceTracker,
    pub source_reliability_evaluator: KnowledgeSourceReliabilityEvaluator,
    pub aging_engine: KnowledgeAgingEngine,
    pub reinforcement_engine: KnowledgeReinforcementEngine,
    pub semantic_cluster_engine: SemanticClusterEngine,
    pub pruning_engine: KnowledgePruningEngine,
    pub quality_monitor: KnowledgeQualityMonitor,
    pub entropy_monitor: KnowledgeEntropyMonitor,
    pub turnover_monitor: KnowledgeTurnoverMonitor,
    pub half_life_monitor: KnowledgeHalfLifeMonitor,
    pub conflict_resolver: KnowledgeConflictResolver,
}

impl Default for KnowledgeLifecycleEngine {
    fn default() -> Self {
        Self::new(
            KnowledgeLifecycleConfig::default(),
            ValidationScore::default(),
            0,
            false,
        )
    }
}

impl KnowledgeLifecycleEngine {
    pub fn new(
        config: KnowledgeLifecycleConfig,
        validation: ValidationScore,
        architecture_usage_count: u64,
        consistent_inference: bool,
    ) -> Self {
        Self {
            provenance_tracker: KnowledgeProvenanceTracker {
                current_cycle: config.current_cycle,
            },
            source_reliability_evaluator: KnowledgeSourceReliabilityEvaluator,
            aging_engine: KnowledgeAgingEngine {
                decay_rate: config.decay_rate,
                current_cycle: config.current_cycle,
            },
            reinforcement_engine: KnowledgeReinforcementEngine {
                config: ReinforcementConfig {
                    reinforcement_rate: config.reinforcement_rate,
                    max_confidence: config.max_confidence,
                },
                evaluation_threshold: config.evaluation_threshold,
                frequent_usage_threshold: config.frequent_usage_threshold,
                evaluation_score: validation.confidence,
                architecture_usage_count,
                consistent_inference,
            },
            semantic_cluster_engine: SemanticClusterEngine {
                similarity_threshold: config.similarity_threshold,
            },
            pruning_engine: KnowledgePruningEngine {
                confidence_threshold: config.prune_confidence_threshold,
                unused_cycles: config.prune_unused_cycles,
                current_cycle: config.current_cycle,
            },
            quality_monitor: KnowledgeQualityMonitor,
            entropy_monitor: KnowledgeEntropyMonitor {
                entropy_threshold: config.entropy_threshold,
            },
            turnover_monitor: KnowledgeTurnoverMonitor,
            half_life_monitor: KnowledgeHalfLifeMonitor {
                current_cycle: config.current_cycle,
            },
            conflict_resolver: KnowledgeConflictResolver,
        }
    }

    pub fn process(&self, graph: &mut KnowledgeGraph) -> KnowledgeLifecycleState {
        self.process_with_context(graph, &ConflictContext::default())
    }

    pub fn process_with_context(
        &self,
        graph: &mut KnowledgeGraph,
        context: &ConflictContext,
    ) -> KnowledgeLifecycleState {
        let previous_relations = graph.relations.len();
        let mut state = KnowledgeLifecycleState {
            cycle: self.provenance_tracker.current_cycle,
            reinforcement_rate_applied: self.reinforcement_engine.config.reinforcement_rate,
            ..KnowledgeLifecycleState::default()
        };

        state.source_reliabilities = self.source_reliability_evaluator.evaluate(graph);
        state.reliability_evaluated = state.source_reliabilities.len();
        state.embeddings_generated = graph.relations.len();

        for relation in &mut graph.relations {
            self.provenance_tracker.record(relation);
            state.provenance_recorded += 1;

            let before_aging = relation.confidence.effective_confidence;
            self.aging_engine.decay_confidence(relation);
            if relation.confidence.effective_confidence < before_aging {
                state.aged_relations += 1;
            }

            if self.reinforcement_engine.reinforce(relation) {
                state.reinforced_relations += 1;
            }
        }

        state.semantic_clusters = self.semantic_cluster_engine.cluster(graph).len();
        state.semantic_pruned_relations = self.semantic_cluster_engine.prune(graph);

        let conflicts = self.conflict_resolver.detect(graph);
        if !conflicts.is_empty() {
            let resolution = self.conflict_resolver.resolve(conflicts, context);
            state.conflicts_resolved = resolution.resolved_count;
            if !resolution.resolved_relations.is_empty() {
                graph.relations.retain(|relation| {
                    !resolution.resolved_relations.iter().any(|winner| {
                        winner.source == relation.source
                            && winner.target == relation.target
                            && winner.relation_type != relation.relation_type
                    })
                });
                graph.relations.extend(resolution.resolved_relations);
                graph
                    .relations
                    .sort_by(|lhs, rhs| relation_key(lhs).cmp(&relation_key(rhs)));
                graph
                    .relations
                    .dedup_by(|lhs, rhs| relation_key(lhs) == relation_key(rhs));
            }
        }

        state.pruned_relations = self.pruning_engine.prune(graph) + state.semantic_pruned_relations;
        state.quality_metrics = self.quality_monitor.analyze(graph);

        let entropy = self.entropy_monitor.calculate(graph);
        if self.entropy_monitor.is_collapse(&entropy) {
            state.diversification_triggered = true;
            state.exploration_weight = 1.25;
            state.reinforcement_rate_applied *= 0.5;
        }

        state.turnover_metrics = self.turnover_monitor.analyze(
            previous_relations,
            graph.relations.len(),
            state
                .pruned_relations
                .saturating_sub(state.semantic_pruned_relations),
        );
        state.half_life = self.half_life_monitor.calculate(graph);
        state.lifecycle_metrics = LifecycleMetrics {
            entropy: entropy.entropy_score,
            average_confidence: state.quality_metrics.average_confidence,
            pruning_rate: state.pruned_relations as f64 / graph.relations.len().max(1) as f64,
            reinforcement_rate: state.reinforced_relations as f64
                / graph.relations.len().max(1) as f64,
            turnover_rate: state.turnover_metrics.turnover_rate,
            half_life: state.half_life.half_life,
        };
        state
    }
}

pub fn generate_embedding(relation: &KnowledgeRelation) -> KnowledgeEmbedding {
    KnowledgeEmbedding {
        vector: vec![
            relation.source.0 as f32 / 32.0,
            relation.target.0 as f32 / 32.0,
            relation_type_rank(relation.relation_type) as f32 * 3.0,
            relation.confidence.effective_confidence as f32,
        ],
    }
}

fn relation_type_rank(relation_type: RelationType) -> u8 {
    match relation_type {
        RelationType::Supports => 1,
        RelationType::Requires => 2,
        RelationType::Constrains => 3,
        RelationType::Recommends => 4,
    }
}

fn cosine_similarity(lhs: &[f32], rhs: &[f32]) -> f32 {
    let dot = lhs.iter().zip(rhs).map(|(l, r)| l * r).sum::<f32>();
    let lhs_norm = lhs.iter().map(|value| value * value).sum::<f32>().sqrt();
    let rhs_norm = rhs.iter().map(|value| value * value).sum::<f32>().sqrt();
    if lhs_norm == 0.0 || rhs_norm == 0.0 {
        0.0
    } else {
        dot / (lhs_norm * rhs_norm)
    }
}

fn relation_key(relation: &KnowledgeRelation) -> (u64, u64, u8) {
    (
        relation.source.0,
        relation.target.0,
        relation_type_rank(relation.relation_type),
    )
}

fn context_score(
    relation: &KnowledgeRelation,
    semantic_labels: &BTreeSet<String>,
    architecture_roles: &BTreeSet<String>,
) -> f64 {
    let relation_label = format!("{:?}", relation.relation_type).to_ascii_lowercase();
    let semantic_relevance = semantic_labels
        .iter()
        .filter(|label| relation_label.contains(label.as_str()) || label.contains(&relation_label))
        .count() as f64;
    let architecture_relevance = architecture_roles
        .iter()
        .filter(|role| relation_label.contains(role.as_str()) || role.contains(&relation_label))
        .count() as f64;
    relation.confidence.effective_confidence
        + relation.confidence.source_reliability
        + semantic_relevance * 0.1
        + architecture_relevance * 0.1
}
