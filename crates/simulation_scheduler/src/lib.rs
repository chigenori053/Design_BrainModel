use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use architecture_domain::{ArchitectureState, ComponentRole};
use design_domain::{Architecture, ClassUnit, Dependency, DesignUnit, Layer, StructureUnit};
use world_model::{DefaultSimulationEngine, SimulationEngine};
use world_model_core::{SimulationResult, WorldState};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SimulationSchedulerConfig {
    pub max_full_simulations: usize,
    pub light_simulation_threshold: f32,
    pub knowledge_threshold: f32,
}

impl Default for SimulationSchedulerConfig {
    fn default() -> Self {
        Self {
            max_full_simulations: 10,
            light_simulation_threshold: 0.45,
            knowledge_threshold: 0.40,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KnowledgeScore {
    pub value: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LightSimulationResult {
    pub feasibility_score: f64,
    pub resource_load: f64,
    pub dependency_health: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchedulerTelemetryEvent {
    CandidateFiltered(String),
    KnowledgeEvaluated(String),
    LightSimulationCompleted(String),
    SimulationScheduled(String),
    SimulationCacheHit(String),
    SimulationCacheMiss(String),
    IncrementalSimulationExecuted(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct LightSimulationTrace {
    pub architecture_hash: String,
    pub knowledge_score: f64,
    pub light_simulation: LightSimulationResult,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SimulationSchedulerTrace {
    pub filtered_candidates: usize,
    pub knowledge_evaluated: usize,
    pub light_simulated: usize,
    pub scheduled_candidates: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub full_simulations: usize,
    pub telemetry_events: Vec<SchedulerTelemetryEvent>,
}

impl Default for SimulationSchedulerTrace {
    fn default() -> Self {
        Self {
            filtered_candidates: 0,
            knowledge_evaluated: 0,
            light_simulated: 0,
            scheduled_candidates: 0,
            cache_hits: 0,
            cache_misses: 0,
            full_simulations: 0,
            telemetry_events: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScheduledCandidate {
    pub architecture_hash: String,
    pub architecture: ArchitectureState,
    pub knowledge_score: KnowledgeScore,
    pub light_simulation: LightSimulationResult,
    pub ranking_score: f64,
    pub simulation_result: SimulationResult,
    pub cache_hit: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ScheduledSimulationBatch {
    pub scheduled: Vec<ScheduledCandidate>,
    pub light_traces: Vec<LightSimulationTrace>,
    pub trace: SimulationSchedulerTrace,
}

pub trait CandidateFilter {
    fn filter(&self, candidates: Vec<ArchitectureState>) -> Vec<ArchitectureState>;
}

pub trait KnowledgeEvaluator {
    fn evaluate(&self, architecture: &ArchitectureState) -> KnowledgeScore;
}

pub trait LightSimulationEngine {
    fn simulate(&self, architecture: &ArchitectureState) -> LightSimulationResult;
}

pub trait SimulationCache {
    fn get(&self, hash: &str) -> Option<SimulationResult>;
    fn store(&self, hash: String, result: SimulationResult);
}

pub trait IncrementalSimulation {
    fn simulate_delta(
        &self,
        base: &ArchitectureState,
        modified: &ArchitectureState,
    ) -> SimulationResult;
}

pub trait SimulationScheduler {
    fn schedule(&self, candidates: Vec<ArchitectureState>) -> Vec<SimulationResult>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultCandidateFilter;

impl DefaultCandidateFilter {
    pub fn is_valid(&self, architecture: &ArchitectureState) -> bool {
        let constraints_ok = architecture.constraints.iter().all(|constraint| {
            constraint
                .max_design_units
                .map(|limit| architecture.metrics.component_count <= limit)
                .unwrap_or(true)
                && constraint
                    .max_dependencies
                    .map(|limit| architecture.metrics.dependency_count <= limit)
                    .unwrap_or(true)
        });
        let no_self_dependency = architecture
            .dependencies
            .iter()
            .all(|dependency| dependency.from != dependency.to);
        let mut reverse_edges = HashSet::new();
        let no_bidirectional_cycle = architecture.dependencies.iter().all(|dependency| {
            let edge = (dependency.from.0, dependency.to.0);
            let reverse = (dependency.to.0, dependency.from.0);
            let valid = !reverse_edges.contains(&reverse);
            reverse_edges.insert(edge);
            valid
        });
        let resource_ok =
            architecture.deployment.replicas <= architecture.metrics.component_count.max(1) * 4;

        constraints_ok && no_self_dependency && no_bidirectional_cycle && resource_ok
    }
}

impl CandidateFilter for DefaultCandidateFilter {
    fn filter(&self, candidates: Vec<ArchitectureState>) -> Vec<ArchitectureState> {
        candidates
            .into_iter()
            .filter(|candidate| self.is_valid(candidate))
            .collect()
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HeuristicKnowledgeEvaluator;

impl KnowledgeEvaluator for HeuristicKnowledgeEvaluator {
    fn evaluate(&self, architecture: &ArchitectureState) -> KnowledgeScore {
        let has_repository = architecture
            .components
            .iter()
            .any(|component| matches!(component.role, ComponentRole::Repository));
        let has_database = architecture
            .components
            .iter()
            .any(|component| matches!(component.role, ComponentRole::Database));
        let pattern_alignment = match (has_repository, has_database) {
            (true, true) => 1.0,
            (true, false) | (false, true) => 0.7,
            (false, false) => 0.4,
        };
        let dependency_density = if architecture.metrics.component_count == 0 {
            0.0
        } else {
            architecture.metrics.dependency_count as f64
                / architecture.metrics.component_count as f64
        };
        let anti_pattern_penalty = dependency_density.min(1.0) * 0.3;
        KnowledgeScore {
            value: (architecture.metrics.layering_score * 0.6 + pattern_alignment * 0.4
                - anti_pattern_penalty)
                .clamp(0.0, 1.0),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct HeuristicLightSimulationEngine;

impl LightSimulationEngine for HeuristicLightSimulationEngine {
    fn simulate(&self, architecture: &ArchitectureState) -> LightSimulationResult {
        let component_count = architecture.metrics.component_count.max(1) as f64;
        let resource_load = (architecture.deployment.replicas as f64 / component_count).max(0.0);
        let dependency_health =
            (1.0 - architecture.metrics.dependency_count as f64 / (component_count * 2.0))
                .clamp(0.0, 1.0);
        let feasibility_score = (architecture.metrics.layering_score * 0.5
            + dependency_health * 0.35
            + (1.0 - resource_load.min(1.0)) * 0.15)
            .clamp(0.0, 1.0);

        LightSimulationResult {
            feasibility_score,
            resource_load,
            dependency_health,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct InMemorySimulationCache {
    results: Arc<Mutex<HashMap<String, SimulationResult>>>,
}

impl SimulationCache for InMemorySimulationCache {
    fn get(&self, hash: &str) -> Option<SimulationResult> {
        self.results
            .lock()
            .expect("simulation cache poisoned")
            .get(hash)
            .cloned()
    }

    fn store(&self, hash: String, result: SimulationResult) {
        self.results
            .lock()
            .expect("simulation cache poisoned")
            .insert(hash, result);
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DeterministicIncrementalSimulation;

impl IncrementalSimulation for DeterministicIncrementalSimulation {
    fn simulate_delta(
        &self,
        base: &ArchitectureState,
        modified: &ArchitectureState,
    ) -> SimulationResult {
        let component_delta = modified.metrics.component_count as f64 - base.metrics.component_count as f64;
        let dependency_delta =
            modified.metrics.dependency_count as f64 - base.metrics.dependency_count as f64;
        let performance_score = (1.0 - dependency_delta.max(0.0) / 10.0).clamp(0.0, 1.0);
        let correctness_score = modified.metrics.layering_score.clamp(0.0, 1.0);
        let constraint_score = if modified.constraints.is_empty() {
            1.0
        } else {
            modified
                .constraints
                .iter()
                .filter(|constraint| {
                    constraint
                        .max_design_units
                        .map(|limit| modified.metrics.component_count <= limit)
                        .unwrap_or(true)
                        && constraint
                            .max_dependencies
                            .map(|limit| modified.metrics.dependency_count <= limit)
                            .unwrap_or(true)
                })
                .count() as f64
                / modified.constraints.len() as f64
        };
        let confidence_score =
            (1.0 - component_delta.abs() / modified.metrics.component_count.max(1) as f64)
                .clamp(0.0, 1.0);
        synthesize_simulation_result(
            performance_score,
            correctness_score,
            constraint_score,
            confidence_score,
            modified,
        )
    }
}

#[derive(Clone, Debug)]
pub struct DefaultSimulationScheduler {
    pub config: SimulationSchedulerConfig,
    pub filter: DefaultCandidateFilter,
    pub knowledge_evaluator: HeuristicKnowledgeEvaluator,
    pub light_engine: HeuristicLightSimulationEngine,
    pub cache: InMemorySimulationCache,
    pub incremental: DeterministicIncrementalSimulation,
}

impl Default for DefaultSimulationScheduler {
    fn default() -> Self {
        Self {
            config: SimulationSchedulerConfig::default(),
            filter: DefaultCandidateFilter,
            knowledge_evaluator: HeuristicKnowledgeEvaluator,
            light_engine: HeuristicLightSimulationEngine,
            cache: InMemorySimulationCache::default(),
            incremental: DeterministicIncrementalSimulation,
        }
    }
}

impl DefaultSimulationScheduler {
    pub fn with_config(config: SimulationSchedulerConfig) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    pub fn architecture_hash(&self, architecture: &ArchitectureState) -> String {
        architecture_hash(architecture)
    }

    pub fn rank_candidates(&self, candidates: Vec<ArchitectureState>) -> ScheduledSimulationBatch {
        let mut trace = SimulationSchedulerTrace::default();
        let original_count = candidates.len();
        let filtered = self.filter.filter(candidates);
        trace.filtered_candidates = original_count.saturating_sub(filtered.len());

        let mut scored = Vec::new();
        let mut light_traces = Vec::new();
        for architecture in filtered {
            let hash = self.architecture_hash(&architecture);
            trace
                .telemetry_events
                .push(SchedulerTelemetryEvent::CandidateFiltered(hash.clone()));
            let knowledge_score = self.knowledge_evaluator.evaluate(&architecture);
            trace.knowledge_evaluated += 1;
            trace
                .telemetry_events
                .push(SchedulerTelemetryEvent::KnowledgeEvaluated(hash.clone()));
            if knowledge_score.value < self.config.knowledge_threshold as f64 {
                continue;
            }
            let light_simulation = self.light_engine.simulate(&architecture);
            trace.light_simulated += 1;
            trace.telemetry_events.push(
                SchedulerTelemetryEvent::LightSimulationCompleted(hash.clone()),
            );
            light_traces.push(LightSimulationTrace {
                architecture_hash: hash.clone(),
                knowledge_score: knowledge_score.value,
                light_simulation,
            });
            if light_simulation.feasibility_score
                < self.config.light_simulation_threshold as f64
            {
                continue;
            }
            let ranking_score =
                knowledge_score.value * 0.45 + light_simulation.feasibility_score * 0.55;
            scored.push((hash, architecture, knowledge_score, light_simulation, ranking_score));
        }

        scored.sort_by(|lhs, rhs| {
            rhs.4
                .total_cmp(&lhs.4)
                .then_with(|| lhs.0.cmp(&rhs.0))
        });

        let mut scheduled = Vec::new();
        for (hash, architecture, knowledge_score, light_simulation, ranking_score) in scored
            .into_iter()
            .take(self.config.max_full_simulations.max(1))
        {
            trace
                .telemetry_events
                .push(SchedulerTelemetryEvent::SimulationScheduled(hash.clone()));
            let (simulation_result, cache_hit) = if let Some(cached) = self.cache.get(&hash) {
                trace.cache_hits += 1;
                trace
                    .telemetry_events
                    .push(SchedulerTelemetryEvent::SimulationCacheHit(hash.clone()));
                (cached, true)
            } else {
                trace.cache_misses += 1;
                trace
                    .telemetry_events
                    .push(SchedulerTelemetryEvent::SimulationCacheMiss(hash.clone()));
                let result = full_simulation_from_architecture(&architecture);
                self.cache.store(hash.clone(), result.clone());
                (result, false)
            };
            trace.full_simulations += 1;
            scheduled.push(ScheduledCandidate {
                architecture_hash: hash,
                architecture,
                knowledge_score,
                light_simulation,
                ranking_score,
                simulation_result,
                cache_hit,
            });
        }

        trace.scheduled_candidates = scheduled.len();
        ScheduledSimulationBatch {
            scheduled,
            light_traces,
            trace,
        }
    }

    pub fn simulate_incrementally(
        &self,
        base: &ArchitectureState,
        modified: &ArchitectureState,
    ) -> (SimulationResult, SchedulerTelemetryEvent) {
        let hash = self.architecture_hash(modified);
        (
            self.incremental.simulate_delta(base, modified),
            SchedulerTelemetryEvent::IncrementalSimulationExecuted(hash),
        )
    }
}

impl SimulationScheduler for DefaultSimulationScheduler {
    fn schedule(&self, candidates: Vec<ArchitectureState>) -> Vec<SimulationResult> {
        self.rank_candidates(candidates)
            .scheduled
            .into_iter()
            .map(|candidate| candidate.simulation_result)
            .collect()
    }
}

pub fn architecture_hash(architecture: &ArchitectureState) -> String {
    let mut roles = architecture
        .components
        .iter()
        .map(|component| format!("{:?}-{}", component.role, component.id.0))
        .collect::<Vec<_>>();
    roles.sort();
    let mut dependencies = architecture
        .dependencies
        .iter()
        .map(|dependency| format!("{}-{}-{:?}", dependency.from.0, dependency.to.0, dependency.kind))
        .collect::<Vec<_>>();
    dependencies.sort();
    format!(
        "components:{}|deps:{}|replicas:{}",
        roles.join(","),
        dependencies.join(","),
        architecture.deployment.replicas
    )
}

fn full_simulation_from_architecture(architecture: &ArchitectureState) -> SimulationResult {
    let world_state = WorldState::from_architecture(
        deterministic_state_id(architecture),
        design_architecture_from_state(architecture),
        architecture.constraints.clone(),
    );
    DefaultSimulationEngine.simulate(&world_state, None)
}

fn deterministic_state_id(architecture: &ArchitectureState) -> u64 {
    architecture
        .components
        .iter()
        .fold(17_u64, |acc, component| acc.wrapping_mul(31).wrapping_add(component.id.0))
}

fn design_architecture_from_state(state: &ArchitectureState) -> Architecture {
    let mut architecture = Architecture {
        classes: vec![ClassUnit::new(1, "Phase30Class")],
        dependencies: Vec::new(),
        graph: Default::default(),
    };
    architecture
        .classes[0]
        .structures
        .push(StructureUnit::new(1, "phase30_structure"));

    for component in &state.components {
        architecture.classes[0]
            .structures[0]
            .design_units
            .push(DesignUnit::with_layer(
                component.id.0,
                format!("{:?}{}", component.role, component.id.0),
                layer_from_role(&component.role),
            ));
    }

    for dependency in &state.dependencies {
        architecture.dependencies.push(Dependency {
            from: dependency.from,
            to: dependency.to,
            kind: dependency.kind,
        });
        architecture
            .graph
            .edges
            .push((dependency.from.0, dependency.to.0));
    }

    architecture
}

fn layer_from_role(role: &ComponentRole) -> Layer {
    match role {
        ComponentRole::Controller => Layer::Ui,
        ComponentRole::Service | ComponentRole::Gateway | ComponentRole::Unknown(_) => {
            Layer::Service
        }
        ComponentRole::Repository => Layer::Repository,
        ComponentRole::Database => Layer::Database,
    }
}

fn synthesize_simulation_result(
    performance_score: f64,
    correctness_score: f64,
    constraint_score: f64,
    confidence_score: f64,
    architecture: &ArchitectureState,
) -> SimulationResult {
    SimulationResult {
        performance_score,
        correctness_score,
        constraint_score,
        confidence_score,
        system: world_model_core::SystemModelMetrics {
            dependency_cycles: architecture
                .dependencies
                .iter()
                .filter(|dependency| {
                    architecture.dependencies.iter().any(|other| {
                        other.from == dependency.to && other.to == dependency.from
                    })
                })
                .count()
                / 2,
            module_coupling: architecture.metrics.layering_score,
            layering_score: architecture.metrics.layering_score,
            call_edges: architecture.metrics.dependency_count,
        },
        math: world_model_core::MathModelMetrics {
            algebraic_score: confidence_score,
            logic_score: correctness_score,
            constraint_solver_score: constraint_score,
        },
        geometry: world_model_core::GeometryModelMetrics {
            graph_layout_score: correctness_score,
            layout_balance_score: confidence_score,
            spatial_constraint_score: constraint_score,
        },
        execution: world_model_core::ExecutionModelMetrics {
            runtime_complexity: architecture.metrics.dependency_count as f64,
            memory_usage: architecture.metrics.component_count as f64,
            dependency_cost: (architecture.metrics.dependency_count as f64
                / architecture.metrics.component_count.max(1) as f64)
                .clamp(0.0, 1.0),
            latency_score: performance_score,
        },
    }
}
