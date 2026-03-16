use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use architecture_ir::{ArchitectureIR, ComponentType, NodeId};

use crate::grammar::{ArchitectureGrammar, ComponentRule, InterfaceRule};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArchitectureGrammarEngine;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GrammarValidation {
    pub valid: bool,
    pub issues: Vec<String>,
}

impl ArchitectureGrammarEngine {
    pub fn validate(&self, ir: &ArchitectureIR, grammar: &ArchitectureGrammar) -> GrammarValidation {
        let mut issues = Vec::new();

        issues.extend(self.validate_components(ir, grammar));
        issues.extend(self.validate_dependencies(ir, grammar));
        issues.extend(self.validate_layers(ir, grammar));
        issues.extend(self.validate_interfaces(ir, grammar));
        issues.extend(self.validate_constraints(ir, grammar));

        GrammarValidation {
            valid: issues.is_empty(),
            issues,
        }
    }

    fn validate_components(&self, ir: &ArchitectureIR, grammar: &ArchitectureGrammar) -> Vec<String> {
        let allowed = grammar
            .component_rules
            .iter()
            .map(|rule| rule.component_type.clone())
            .collect::<Vec<_>>();

        ir.components
            .iter()
            .filter(|component| !allowed.contains(&component.component_type))
            .map(|component| {
                format!(
                    "component '{}' of type {:?} is not allowed by grammar",
                    component.name, component.component_type
                )
            })
            .collect()
    }

    fn validate_dependencies(
        &self,
        ir: &ArchitectureIR,
        grammar: &ArchitectureGrammar,
    ) -> Vec<String> {
        let component_rules = grammar
            .component_rules
            .iter()
            .map(|rule| (rule.component_type.clone(), rule))
            .collect::<HashMap<_, _>>();
        let mut issues = Vec::new();

        for dependency in &ir.dependencies {
            let (NodeId::Component(from_id), NodeId::Component(to_id)) =
                (dependency.source, dependency.target)
            else {
                continue;
            };
            let Some(from) = ir.component_by_id(from_id) else {
                continue;
            };
            let Some(to) = ir.component_by_id(to_id) else {
                continue;
            };

            let allowed = grammar.dependency_rules.iter().any(|rule| {
                rule.from == from.component_type && rule.to == to.component_type
            });
            if !allowed {
                issues.push(format!(
                    "forbidden dependency: {:?} -> {:?}",
                    from.component_type, to.component_type
                ));
            }

            if let Some(rule) = component_rules.get(&from.component_type) {
                if !rule.allowed_dependencies.contains(&to.component_type) {
                    issues.push(format!(
                        "component rule violation: {:?} cannot depend on {:?}",
                        from.component_type, to.component_type
                    ));
                }
            }
        }

        issues
    }

    fn validate_layers(&self, ir: &ArchitectureIR, grammar: &ArchitectureGrammar) -> Vec<String> {
        let layer_levels = grammar
            .layer_rules
            .iter()
            .map(|rule| (rule.name.clone(), rule.level))
            .collect::<BTreeMap<_, _>>();
        let component_layers = grammar
            .component_rules
            .iter()
            .map(|rule| (rule.component_type.clone(), rule.layer.clone()))
            .collect::<HashMap<_, _>>();

        let mut issues = Vec::new();
        for dependency in &ir.dependencies {
            let (NodeId::Component(from_id), NodeId::Component(to_id)) =
                (dependency.source, dependency.target)
            else {
                continue;
            };
            let Some(from) = ir.component_by_id(from_id) else {
                continue;
            };
            let Some(to) = ir.component_by_id(to_id) else {
                continue;
            };

            let Some(from_layer) = component_layers.get(&from.component_type) else {
                continue;
            };
            let Some(to_layer) = component_layers.get(&to.component_type) else {
                continue;
            };
            let Some(from_level) = layer_levels.get(from_layer) else {
                continue;
            };
            let Some(to_level) = layer_levels.get(to_layer) else {
                continue;
            };

            if from_level < to_level {
                issues.push(format!(
                    "layer violation: {} ({:?}) cannot depend on {} ({:?})",
                    from_layer, from.component_type, to_layer, to.component_type
                ));
            }
        }

        issues
    }

    fn validate_interfaces(
        &self,
        ir: &ArchitectureIR,
        grammar: &ArchitectureGrammar,
    ) -> Vec<String> {
        let mut issues = Vec::new();
        let present_types = ir
            .components
            .iter()
            .map(|component| component.component_type.clone())
            .collect::<HashSet<_>>();

        for rule in &grammar.interface_rules {
            if !rule.required {
                continue;
            }
            if !present_types.contains(&rule.exposer) {
                continue;
            }
            if !rule.implementors.iter().all(|ty| present_types.contains(ty)) {
                continue;
            }
            if !present_types.contains(&rule.interface_type) {
                issues.push(format!(
                    "interface rule violation: {:?} requires {:?} between exposer and implementor",
                    rule.exposer, rule.interface_type
                ));
            }
        }

        issues
    }

    fn validate_constraints(
        &self,
        ir: &ArchitectureIR,
        grammar: &ArchitectureGrammar,
    ) -> Vec<String> {
        let mut issues = Vec::new();

        let dependency_counts = ir.dependencies.iter().fold(BTreeMap::new(), |mut acc, edge| {
            if let NodeId::Component(from) = edge.source {
                *acc.entry(from).or_insert(0usize) += 1;
            }
            acc
        });
        if dependency_counts
            .values()
            .any(|count| *count > grammar.constraint_rule.max_dependencies_per_component)
        {
            issues.push("max_dependencies_per_component exceeded".to_string());
        }

        let layer_count = grammar
            .component_rules
            .iter()
            .filter(|rule| {
                ir.components
                    .iter()
                    .any(|component| component.component_type == rule.component_type)
            })
            .map(|rule| rule.layer.clone())
            .collect::<BTreeSet<_>>()
            .len();
        if layer_count > grammar.constraint_rule.max_layer_depth {
            issues.push("max_layer_depth exceeded".to_string());
        }

        if grammar.constraint_rule.no_circular_dependency && has_cycle(ir) {
            issues.push("no_circular_dependency violated".to_string());
        }

        issues
    }
}

fn has_cycle(ir: &ArchitectureIR) -> bool {
    let mut adjacency = BTreeMap::<u64, Vec<u64>>::new();
    for dependency in &ir.dependencies {
        if let (NodeId::Component(from), NodeId::Component(to)) = (dependency.source, dependency.target)
        {
            adjacency.entry(from).or_default().push(to);
            adjacency.entry(to).or_default();
        }
    }

    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();

    adjacency
        .keys()
        .copied()
        .any(|node| detect_cycle(node, &adjacency, &mut visiting, &mut visited))
}

fn detect_cycle(
    node: u64,
    adjacency: &BTreeMap<u64, Vec<u64>>,
    visiting: &mut BTreeSet<u64>,
    visited: &mut BTreeSet<u64>,
) -> bool {
    if visited.contains(&node) {
        return false;
    }
    if !visiting.insert(node) {
        return true;
    }

    let cyclic = adjacency
        .get(&node)
        .into_iter()
        .flatten()
        .copied()
        .any(|next| detect_cycle(next, adjacency, visiting, visited));
    visiting.remove(&node);
    visited.insert(node);
    cyclic
}

pub(crate) fn default_component_rule(
    name: &str,
    component_type: ComponentType,
    layer: &str,
    allowed_dependencies: Vec<ComponentType>,
) -> ComponentRule {
    ComponentRule {
        name: name.to_string(),
        component_type,
        layer: layer.to_string(),
        allowed_dependencies,
        required_interfaces: Vec::new(),
    }
}

pub(crate) fn default_interface_rule(
    exposer: ComponentType,
    interface_type: ComponentType,
    implementors: Vec<ComponentType>,
    required: bool,
) -> InterfaceRule {
    InterfaceRule {
        exposer,
        interface_type,
        implementors,
        required,
    }
}
