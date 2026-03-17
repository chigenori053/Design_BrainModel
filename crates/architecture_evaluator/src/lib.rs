use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub mod stable_v03;

use architecture_behavior::{BehaviorAnalysis, BehaviorAnalyzer};
use architecture_ir::{architecture_hash, ArchitectureIR, NodeId};
use architecture_knowledge::{KnowledgeAnalyzer, PatternDetection};
use architecture_memory::{recall_similar_architecture, ArchitectureMemory};
use architecture_metrics::{ArchitectureMetrics, MetricsCalculator};
use architecture_rules::{RuleValidator, RuleViolation};
use architecture_state_v2::{ArchitectureEvaluation, ArchitectureState};
use execution_graph::ExecutionGraphBuilder;
use geometry_engine::GeometryEngine;
use memory_space_phase14::{
    embed_evaluation, DesignMemorySpace, EvaluationDiagnostics as MemoryEvaluationDiagnostics,
    EvaluationMetricsV2 as MemoryEvaluationMetricsV2, EvaluationRecord as MemoryEvaluationRecord,
    EvaluationScores as MemoryEvaluationScores,
};
use workload_model::WorkloadModel;

pub trait ArchitectureEvaluator {
    fn evaluate(&self, state: &ArchitectureState) -> ArchitectureEvaluation;

    fn evaluate_score(&self, state: &ArchitectureState) -> ArchitectureScore;
}

pub trait ArchitectureIrEvaluator {
    fn evaluate_ir(&self, architecture: &ArchitectureIR) -> IrEvaluationResult;
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArchitectureScore {
    pub structural: f64,
    pub rule_score: f64,
    pub knowledge_score: f64,
    pub intent_alignment: f64,
}

impl ArchitectureScore {
    pub fn total(&self) -> f64 {
        ((self.structural + self.rule_score + self.knowledge_score + self.intent_alignment) / 4.0)
            .clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationDetails {
    pub score: ArchitectureScore,
    pub metrics: ArchitectureMetrics,
    pub violations: Vec<RuleViolation>,
    pub pattern_detection: PatternDetection,
    pub recalled_patterns: Vec<String>,
    pub behavior: Option<BehaviorAnalysis>,
    pub score_v3: Option<ArchitectureScoreV3>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationScores {
    pub layering_score: f64,
    pub coupling_score: f64,
    pub cohesion_score: f64,
    pub complexity_score: f64,
    pub modularity_score: f64,
    pub overall_score: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationMetricsV2 {
    pub component_count: usize,
    pub dependency_count: usize,
    pub layer_count: usize,
    pub cycle_count: usize,
    pub average_degree: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationDiagnostics {
    pub layer_violations: Vec<String>,
    pub circular_dependencies: Vec<Vec<String>>,
    pub high_coupling_components: Vec<String>,
    pub interface_mismatch: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvaluationTelemetry {
    pub evaluation_time_ms: u64,
    pub metric_calculation_time_ms: u64,
    pub cache_hit: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct IrEvaluationResult {
    pub scores: EvaluationScores,
    pub metrics: EvaluationMetricsV2,
    pub diagnostics: EvaluationDiagnostics,
    pub telemetry: EvaluationTelemetry,
}

#[derive(Debug, Default)]
pub struct ArchitectureEvaluatorEngine {
    cache: Mutex<HashMap<u64, IrEvaluationResult>>,
    memory_space: Option<Arc<Mutex<DesignMemorySpace>>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArchitectureScoreV3 {
    pub structural_score: f64,
    pub rule_score: f64,
    pub knowledge_score: f64,
    pub behavior_score: f64,
}

impl ArchitectureScoreV3 {
    pub fn total(&self) -> f64 {
        ((self.structural_score + self.rule_score + self.knowledge_score + self.behavior_score)
            / 4.0)
            .clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, Default)]
pub struct DefaultArchitectureEvaluator;

impl ArchitectureEvaluator for DefaultArchitectureEvaluator {
    fn evaluate(&self, state: &ArchitectureState) -> ArchitectureEvaluation {
        let geometry = GeometryEngine.evaluate(&state.architecture_graph);
        let details = self.evaluate_details(state, None);

        ArchitectureEvaluation {
            geometry,
            knowledge_alignment: details.score.knowledge_score,
            overall: details.score.total(),
        }
    }

    fn evaluate_score(&self, state: &ArchitectureState) -> ArchitectureScore {
        self.evaluate_details(state, None).score
    }
}

impl DefaultArchitectureEvaluator {
    pub fn evaluate_details(
        &self,
        state: &ArchitectureState,
        memory: Option<&ArchitectureMemory>,
    ) -> EvaluationDetails {
        let metrics = MetricsCalculator.compute(&state.architecture_graph);
        let violations = RuleValidator.validate(&state.architecture_graph);
        let detection = KnowledgeAnalyzer::default().detect(&state.architecture_graph);
        let recalled_patterns = memory
            .map(|memory| recall_similar_architecture(&state.architecture_graph, memory))
            .unwrap_or_default()
            .into_iter()
            .map(|pattern| pattern.name)
            .collect::<Vec<_>>();
        let rule_score = (1.0 - violations.len() as f64 * 0.2).clamp(0.0, 1.0);
        let structural = ((metrics.modularity
            + (1.0 - metrics.coupling)
            + metrics.cohesion
            + metrics.layering_score
            + (1.0 - metrics.dependency_entropy))
            / 5.0)
            .clamp(0.0, 1.0);
        let knowledge_alignment = if let Some(knowledge) = &state.knowledge {
            knowledge.validation.confidence
        } else {
            0.5
        };
        let memory_bonus = if recalled_patterns.is_empty() {
            0.0
        } else {
            0.1
        };
        let intent_alignment = intent_alignment_score(state, &detection, memory_bonus);
        let knowledge_score =
            (detection.knowledge_score + knowledge_alignment + memory_bonus).min(1.0);
        let score = ArchitectureScore {
            structural,
            rule_score,
            knowledge_score,
            intent_alignment,
        };

        EvaluationDetails {
            score,
            metrics,
            violations,
            pattern_detection: detection,
            recalled_patterns,
            behavior: None,
            score_v3: None,
        }
    }

    pub fn evaluate_v3(
        &self,
        state: &ArchitectureState,
        workload: &WorkloadModel,
        memory: Option<&ArchitectureMemory>,
    ) -> EvaluationDetails {
        let mut details = self.evaluate_details(state, memory);
        let execution_graph = ExecutionGraphBuilder.build(&state.architecture_graph);
        let behavior = BehaviorAnalyzer.analyze(&execution_graph, workload);
        let score_v3 = ArchitectureScoreV3 {
            structural_score: details.score.structural,
            rule_score: details.score.rule_score,
            knowledge_score: details.score.knowledge_score,
            behavior_score: behavior.behavior_score,
        };
        details.behavior = Some(behavior);
        details.score_v3 = Some(score_v3);
        details
    }
}

impl ArchitectureIrEvaluator for ArchitectureEvaluatorEngine {
    fn evaluate_ir(&self, architecture: &ArchitectureIR) -> IrEvaluationResult {
        let key = architecture_hash(architecture);
        let hash_key = format!("{:016x}", key);
        if let Some(memory_space) = &self.memory_space {
            if let Some(cached) = memory_space
                .lock()
                .expect("memory space poisoned")
                .find_evaluation(&hash_key)
                .cloned()
            {
                return ir_result_from_memory_record(cached, true);
            }
        }
        if let Some(cached) = self
            .cache
            .lock()
            .expect("evaluation cache poisoned")
            .get(&key)
            .cloned()
        {
            let mut cached = cached;
            cached.telemetry.cache_hit = true;
            return cached;
        }

        let started = Instant::now();
        let metric_started = Instant::now();
        let component_count = architecture.components.len();
        let dependency_count = architecture.dependencies.len();
        let layer_count = architecture.layers.len();
        let average_degree = if component_count == 0 {
            0.0
        } else {
            dependency_count as f64 / component_count as f64
        };
        let layer_violations = detect_ir_layer_violations(architecture);
        let cycles = tarjan_component_cycles(architecture);
        let interface_mismatch = detect_interface_mismatch(architecture);
        let high_coupling_components = architecture
            .components
            .iter()
            .filter_map(|component| {
                let outgoing = architecture
                    .dependencies
                    .iter()
                    .filter(|edge| edge.source == NodeId::Component(component.id))
                    .count();
                (outgoing > 5).then(|| component.name.clone())
            })
            .collect::<Vec<_>>();
        let metric_time = metric_started.elapsed();

        let layering_score = ratio_score(
            dependency_count.saturating_sub(layer_violations.len()),
            dependency_count,
        );
        let coupling_score = (1.0 - normalize_range(average_degree, 0.0, 8.0)).clamp(0.0, 1.0);
        let cohesion_score = compute_cohesion_score(architecture);
        let complexity_score = complexity_score(component_count, dependency_count);
        let modularity_score = compute_modularity_score(architecture);
        let overall_score = weighted_overall_score(
            layering_score,
            coupling_score,
            cohesion_score,
            complexity_score,
            modularity_score,
        );

        let result = IrEvaluationResult {
            scores: EvaluationScores {
                layering_score,
                coupling_score,
                cohesion_score,
                complexity_score,
                modularity_score,
                overall_score,
            },
            metrics: EvaluationMetricsV2 {
                component_count,
                dependency_count,
                layer_count,
                cycle_count: cycles.len(),
                average_degree,
            },
            diagnostics: EvaluationDiagnostics {
                layer_violations,
                circular_dependencies: cycles,
                high_coupling_components,
                interface_mismatch,
            },
            telemetry: EvaluationTelemetry {
                evaluation_time_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
                metric_calculation_time_ms: metric_time.as_millis().try_into().unwrap_or(u64::MAX),
                cache_hit: false,
            },
        };

        self.cache
            .lock()
            .expect("evaluation cache poisoned")
            .insert(key, result.clone());
        if let Some(memory_space) = &self.memory_space {
            let record = memory_record_from_ir_result(&hash_key, &result);
            memory_space
                .lock()
                .expect("memory space poisoned")
                .store_evaluation(record.clone(), embed_evaluation(&record));
        }
        result
    }
}

impl ArchitectureEvaluatorEngine {
    pub fn with_memory_space(memory_space: Arc<Mutex<DesignMemorySpace>>) -> Self {
        Self {
            cache: Mutex::default(),
            memory_space: Some(memory_space),
        }
    }

    pub fn cache_size(&self) -> usize {
        self.cache.lock().expect("evaluation cache poisoned").len()
    }
}

fn memory_record_from_ir_result(hash: &str, result: &IrEvaluationResult) -> MemoryEvaluationRecord {
    MemoryEvaluationRecord {
        architecture_hash: hash.to_string(),
        evaluation_scores: MemoryEvaluationScores {
            layering_score: result.scores.layering_score,
            coupling_score: result.scores.coupling_score,
            cohesion_score: result.scores.cohesion_score,
            complexity_score: result.scores.complexity_score,
            modularity_score: result.scores.modularity_score,
            overall_score: result.scores.overall_score,
        },
        evaluation_metrics: MemoryEvaluationMetricsV2 {
            component_count: result.metrics.component_count,
            dependency_count: result.metrics.dependency_count,
            layer_count: result.metrics.layer_count,
            cycle_count: result.metrics.cycle_count,
            average_degree: result.metrics.average_degree,
        },
        diagnostics: MemoryEvaluationDiagnostics {
            layer_violations: result.diagnostics.layer_violations.clone(),
            circular_dependencies: result.diagnostics.circular_dependencies.clone(),
            high_coupling_components: result.diagnostics.high_coupling_components.clone(),
            interface_mismatch: result.diagnostics.interface_mismatch.clone(),
        },
    }
}

fn ir_result_from_memory_record(
    record: MemoryEvaluationRecord,
    cache_hit: bool,
) -> IrEvaluationResult {
    IrEvaluationResult {
        scores: EvaluationScores {
            layering_score: record.evaluation_scores.layering_score,
            coupling_score: record.evaluation_scores.coupling_score,
            cohesion_score: record.evaluation_scores.cohesion_score,
            complexity_score: record.evaluation_scores.complexity_score,
            modularity_score: record.evaluation_scores.modularity_score,
            overall_score: record.evaluation_scores.overall_score,
        },
        metrics: EvaluationMetricsV2 {
            component_count: record.evaluation_metrics.component_count,
            dependency_count: record.evaluation_metrics.dependency_count,
            layer_count: record.evaluation_metrics.layer_count,
            cycle_count: record.evaluation_metrics.cycle_count,
            average_degree: record.evaluation_metrics.average_degree,
        },
        diagnostics: EvaluationDiagnostics {
            layer_violations: record.diagnostics.layer_violations,
            circular_dependencies: record.diagnostics.circular_dependencies,
            high_coupling_components: record.diagnostics.high_coupling_components,
            interface_mismatch: record.diagnostics.interface_mismatch,
        },
        telemetry: EvaluationTelemetry {
            evaluation_time_ms: 0,
            metric_calculation_time_ms: 0,
            cache_hit,
        },
    }
}

fn intent_alignment_score(
    state: &ArchitectureState,
    detection: &PatternDetection,
    memory_bonus: f64,
) -> f64 {
    let problem = state.problem.to_ascii_lowercase();
    let layered_match = problem.contains("layer") || problem.contains("api");
    let service_match = problem.contains("service") || problem.contains("microservice");
    let pattern_bonus = detection
        .matched_patterns
        .iter()
        .filter(|pattern| {
            let name = pattern.name.to_ascii_lowercase();
            (layered_match && name.contains("layered"))
                || (service_match && name.contains("service"))
        })
        .count() as f64
        * 0.2;
    (0.4 + pattern_bonus + memory_bonus).clamp(0.0, 1.0)
}

fn ratio_score(valid: usize, total: usize) -> f64 {
    if total == 0 {
        1.0
    } else {
        valid as f64 / total as f64
    }
}

fn normalize_range(value: f64, min: f64, max: f64) -> f64 {
    if (max - min).abs() < f64::EPSILON {
        0.0
    } else {
        ((value - min) / (max - min)).clamp(0.0, 1.0)
    }
}

fn complexity_score(nodes: usize, edges: usize) -> f64 {
    let complexity = (nodes + edges).max(1) as f64;
    (1.0 / complexity.ln_1p()).clamp(0.0, 1.0)
}

fn weighted_overall_score(
    layering_score: f64,
    coupling_score: f64,
    cohesion_score: f64,
    complexity_score: f64,
    modularity_score: f64,
) -> f64 {
    (0.25 * layering_score
        + 0.20 * coupling_score
        + 0.20 * cohesion_score
        + 0.15 * complexity_score
        + 0.20 * modularity_score)
        .clamp(0.0, 1.0)
}

fn compute_cohesion_score(architecture: &ArchitectureIR) -> f64 {
    if architecture.dependencies.is_empty() {
        return 1.0;
    }
    let owner_by_interface = architecture
        .interfaces
        .iter()
        .map(|interface| (interface.id, interface.owner_component))
        .collect::<BTreeMap<_, _>>();

    let cohesive_edges = architecture
        .dependencies
        .iter()
        .filter(|edge| match (edge.source, edge.target, edge.interface) {
            (NodeId::Component(source), NodeId::Component(target), Some(interface_id)) => {
                owner_by_interface
                    .get(&interface_id)
                    .map(|owner| *owner == source || *owner == target)
                    .unwrap_or(false)
            }
            (NodeId::Component(source), NodeId::Component(target), None) => {
                component_layer(architecture, source) == component_layer(architecture, target)
            }
            _ => false,
        })
        .count();
    cohesive_edges as f64 / architecture.dependencies.len() as f64
}

fn compute_modularity_score(architecture: &ArchitectureIR) -> f64 {
    if architecture.dependencies.is_empty() {
        return 1.0;
    }
    let same_layer_edges = architecture
        .dependencies
        .iter()
        .filter(|edge| match (edge.source, edge.target) {
            (NodeId::Component(source), NodeId::Component(target)) => {
                component_layer(architecture, source) == component_layer(architecture, target)
            }
            _ => false,
        })
        .count();
    same_layer_edges as f64 / architecture.dependencies.len() as f64
}

fn detect_ir_layer_violations(architecture: &ArchitectureIR) -> Vec<String> {
    architecture
        .dependencies
        .iter()
        .filter_map(|edge| match (edge.source, edge.target) {
            (NodeId::Component(source), NodeId::Component(target)) => {
                let source_layer = component_layer_level(architecture, source)?;
                let target_layer = component_layer_level(architecture, target)?;
                if source_layer < target_layer {
                    Some(format!(
                        "{} -> {}",
                        component_name(architecture, source)?,
                        component_name(architecture, target)?
                    ))
                } else {
                    None
                }
            }
            _ => None,
        })
        .collect()
}

fn detect_interface_mismatch(architecture: &ArchitectureIR) -> Vec<String> {
    let interface_ids = architecture
        .interfaces
        .iter()
        .map(|interface| interface.id)
        .collect::<BTreeSet<_>>();
    architecture
        .dependencies
        .iter()
        .filter_map(|edge| {
            edge.interface.and_then(|interface_id| {
                (!interface_ids.contains(&interface_id)).then(|| {
                    format!(
                        "dependency {} -> {} references missing interface {}",
                        display_node(architecture, edge.source),
                        display_node(architecture, edge.target),
                        interface_id
                    )
                })
            })
        })
        .chain(architecture.interfaces.iter().filter_map(|interface| {
            architecture
                .component_by_id(interface.owner_component)
                .is_none()
                .then(|| {
                    format!(
                        "interface {} has missing owner {}",
                        interface.name, interface.owner_component
                    )
                })
        }))
        .collect()
}

fn tarjan_component_cycles(architecture: &ArchitectureIR) -> Vec<Vec<String>> {
    let adjacency = architecture
        .dependencies
        .iter()
        .filter_map(|edge| match (edge.source, edge.target) {
            (NodeId::Component(source), NodeId::Component(target)) => Some((source, target)),
            _ => None,
        })
        .fold(
            BTreeMap::<u64, Vec<u64>>::new(),
            |mut map, (source, target)| {
                map.entry(source).or_default().push(target);
                map.entry(target).or_default();
                map
            },
        );

    struct TarjanState {
        index: usize,
        stack: Vec<u64>,
        on_stack: BTreeSet<u64>,
        indices: BTreeMap<u64, usize>,
        lowlink: BTreeMap<u64, usize>,
        sccs: Vec<Vec<u64>>,
    }

    fn strong_connect(node: u64, adjacency: &BTreeMap<u64, Vec<u64>>, state: &mut TarjanState) {
        state.indices.insert(node, state.index);
        state.lowlink.insert(node, state.index);
        state.index += 1;
        state.stack.push(node);
        state.on_stack.insert(node);

        if let Some(targets) = adjacency.get(&node) {
            for target in targets {
                if !state.indices.contains_key(target) {
                    strong_connect(*target, adjacency, state);
                    let next_low = state.lowlink[target];
                    let current = state.lowlink[&node];
                    state.lowlink.insert(node, current.min(next_low));
                } else if state.on_stack.contains(target) {
                    let current = state.lowlink[&node];
                    let target_index = state.indices[target];
                    state.lowlink.insert(node, current.min(target_index));
                }
            }
        }

        if state.lowlink[&node] == state.indices[&node] {
            let mut component = Vec::new();
            while let Some(top) = state.stack.pop() {
                state.on_stack.remove(&top);
                component.push(top);
                if top == node {
                    break;
                }
            }
            if component.len() > 1 {
                state.sccs.push(component);
            }
        }
    }

    let mut state = TarjanState {
        index: 0,
        stack: Vec::new(),
        on_stack: BTreeSet::new(),
        indices: BTreeMap::new(),
        lowlink: BTreeMap::new(),
        sccs: Vec::new(),
    };
    for node in adjacency.keys().copied().collect::<Vec<_>>() {
        if !state.indices.contains_key(&node) {
            strong_connect(node, &adjacency, &mut state);
        }
    }

    state
        .sccs
        .into_iter()
        .map(|scc| {
            let mut names = scc
                .into_iter()
                .filter_map(|id| component_name(architecture, id).map(str::to_string))
                .collect::<Vec<_>>();
            names.sort();
            names
        })
        .collect()
}

fn component_layer(architecture: &ArchitectureIR, component_id: u64) -> Option<&str> {
    let layer_id = architecture.component_by_id(component_id)?.layer?;
    architecture
        .layers
        .iter()
        .find(|layer| layer.id == layer_id)
        .map(|layer| layer.name.as_str())
}

fn component_layer_level(architecture: &ArchitectureIR, component_id: u64) -> Option<u32> {
    let layer_id = architecture.component_by_id(component_id)?.layer?;
    architecture
        .layers
        .iter()
        .find(|layer| layer.id == layer_id)
        .map(|layer| layer.level)
}

fn component_name(architecture: &ArchitectureIR, component_id: u64) -> Option<&str> {
    architecture
        .component_by_id(component_id)
        .map(|component| component.name.as_str())
}

fn display_node(architecture: &ArchitectureIR, node: NodeId) -> String {
    match node {
        NodeId::Component(id) => component_name(architecture, id)
            .unwrap_or("unknown")
            .to_string(),
        NodeId::Domain(id) => format!("domain:{id}"),
        NodeId::Structure(id) => format!("structure:{id}"),
    }
}

#[cfg(test)]
mod tests {
    use architecture_reasoner::ReverseArchitectureReasoner;
    use code_ir::CodeIr;
    use design_domain::DesignUnit;

    use super::*;

    #[test]
    fn computes_non_zero_score_for_consistent_architecture() {
        let mut controller = DesignUnit::new(1, "ApiController");
        controller.dependencies.push(design_domain::DesignUnitId(2));
        let service = DesignUnit::new(2, "UserService");
        let code_ir = CodeIr::from_design_units(&[controller, service]);
        let architecture_graph = ReverseArchitectureReasoner.infer_from_code_ir(&code_ir);
        let state = ArchitectureState {
            problem: "serve users".into(),
            code_ir,
            architecture_graph,
            ..ArchitectureState::default()
        };

        let evaluation = DefaultArchitectureEvaluator.evaluate(&state);

        assert!(evaluation.overall > 0.0);
    }
}
