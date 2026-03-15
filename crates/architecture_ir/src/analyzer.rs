use std::collections::BTreeMap;

use petgraph::algo::is_cyclic_directed;

use crate::{
    ArchitectureConstraint, ArchitectureIR, ComponentUnitId, ConstraintType, DependencyEdge, NodeId,
};

pub trait ArchitectureAnalyzer {
    fn analyze(&self, ir: &ArchitectureIR) -> AnalysisResult;
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnalysisResult {
    pub risks: Vec<ArchitectureRisk>,
    pub metrics: ArchitectureMetrics,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchitectureRisk {
    pub description: String,
    pub severity: RiskLevel,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ArchitectureMetrics {
    pub coupling: f32,
    pub cohesion: f32,
    pub layering_score: f32,
    pub complexity_score: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BasicArchitectureAnalyzer;

impl ArchitectureAnalyzer for BasicArchitectureAnalyzer {
    fn analyze(&self, ir: &ArchitectureIR) -> AnalysisResult {
        let graph = ir.to_graph();
        let mut risks = Vec::new();

        if is_cyclic_directed(&graph) {
            risks.push(ArchitectureRisk {
                description: "Circular dependencies detected in the architecture graph."
                    .to_string(),
                severity: risk_for_constraint(
                    &ir.constraints,
                    ConstraintType::NoCircularDependency,
                    RiskLevel::Critical,
                ),
            });
        }

        let layer_violations = detect_layer_violations(ir);
        if !layer_violations.is_empty() {
            risks.push(ArchitectureRisk {
                description: format!(
                    "{} dependency edge(s) violate declared layer ordering.",
                    layer_violations.len()
                ),
                severity: risk_for_constraint(
                    &ir.constraints,
                    ConstraintType::LayerViolation,
                    RiskLevel::High,
                ),
            });
        }

        let high_fan_out = ir
            .components
            .iter()
            .filter(|component| component.metrics.fan_out > 10)
            .count();
        if high_fan_out > 0 {
            risks.push(ArchitectureRisk {
                description: format!(
                    "{} component(s) exceed the default fan-out threshold of 10.",
                    high_fan_out
                ),
                severity: risk_for_constraint(
                    &ir.constraints,
                    ConstraintType::DependencyLimit,
                    RiskLevel::Medium,
                ),
            });
        }

        let high_complexity = ir
            .components
            .iter()
            .filter(|component| component.metrics.cyclomatic_complexity > 15)
            .count();
        if high_complexity > 0 {
            risks.push(ArchitectureRisk {
                description: format!(
                    "{} component(s) exceed the default cyclomatic complexity threshold of 15.",
                    high_complexity
                ),
                severity: risk_for_constraint(
                    &ir.constraints,
                    ConstraintType::ComplexityLimit,
                    RiskLevel::High,
                ),
            });
        }

        AnalysisResult {
            risks,
            metrics: ArchitectureMetrics {
                coupling: coupling_score(ir),
                cohesion: cohesion_score(ir),
                layering_score: layering_score(ir, &layer_violations),
                complexity_score: complexity_score(ir),
            },
        }
    }
}

fn risk_for_constraint(
    constraints: &[ArchitectureConstraint],
    kind: ConstraintType,
    default: RiskLevel,
) -> RiskLevel {
    if constraints
        .iter()
        .any(|constraint| constraint.constraint_type == kind)
    {
        match kind {
            ConstraintType::NoCircularDependency => RiskLevel::Critical,
            ConstraintType::LayerViolation => RiskLevel::High,
            ConstraintType::DependencyLimit => RiskLevel::Medium,
            ConstraintType::ComplexityLimit => RiskLevel::High,
        }
    } else {
        default
    }
}

fn detect_layer_violations(ir: &ArchitectureIR) -> Vec<&DependencyEdge> {
    let component_to_level = layer_levels(ir);
    ir.dependencies
        .iter()
        .filter(|edge| {
            match (
                component_level(&component_to_level, edge.source),
                component_level(&component_to_level, edge.target),
            ) {
                (Some(source), Some(target)) => source < target,
                _ => false,
            }
        })
        .collect()
}

fn layer_levels(ir: &ArchitectureIR) -> BTreeMap<ComponentUnitId, u32> {
    let mut levels = BTreeMap::new();
    for layer in &ir.layers {
        for component_id in &layer.components {
            levels.insert(*component_id, layer.level);
        }
    }
    levels
}

fn coupling_score(ir: &ArchitectureIR) -> f32 {
    if ir.components.is_empty() {
        return 0.0;
    }
    let total_fan_out: u32 = ir
        .components
        .iter()
        .map(|component| component.metrics.fan_out)
        .sum();
    total_fan_out as f32 / ir.components.len() as f32
}

fn cohesion_score(ir: &ArchitectureIR) -> f32 {
    if ir.dependencies.is_empty() {
        return 1.0;
    }
    let component_to_layer = layer_name_map(ir);
    let same_layer = ir
        .dependencies
        .iter()
        .filter(|edge| {
            match (
                layer_name(&component_to_layer, edge.source),
                layer_name(&component_to_layer, edge.target),
            ) {
                (Some(source), Some(target)) => source == target,
                _ => false,
            }
        })
        .count();
    same_layer as f32 / ir.dependencies.len() as f32
}

fn layering_score(ir: &ArchitectureIR, layer_violations: &[&DependencyEdge]) -> f32 {
    if ir.dependencies.is_empty() {
        return 1.0;
    }
    1.0 - (layer_violations.len() as f32 / ir.dependencies.len() as f32)
}

fn complexity_score(ir: &ArchitectureIR) -> f32 {
    if ir.components.is_empty() {
        return 0.0;
    }
    let total: u32 = ir
        .components
        .iter()
        .map(|component| component.metrics.cyclomatic_complexity)
        .sum();
    total as f32 / ir.components.len() as f32
}

fn layer_name_map(ir: &ArchitectureIR) -> BTreeMap<ComponentUnitId, &str> {
    let mut map = BTreeMap::new();
    for layer in &ir.layers {
        for component_id in &layer.components {
            map.insert(*component_id, layer.name.as_str());
        }
    }
    map
}

fn component_level(levels: &BTreeMap<ComponentUnitId, u32>, id: NodeId) -> Option<u32> {
    match id {
        NodeId::Component(component_id) => levels.get(&component_id).copied(),
        _ => None,
    }
}

fn layer_name<'a>(layers: &'a BTreeMap<ComponentUnitId, &'a str>, id: NodeId) -> Option<&'a str> {
    match id {
        NodeId::Component(component_id) => layers.get(&component_id).copied(),
        _ => None,
    }
}
