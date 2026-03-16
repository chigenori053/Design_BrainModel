use std::collections::{BTreeMap, BTreeSet};

use petgraph::algo::is_cyclic_directed;

use crate::{ArchitectureIR, ComponentType, NodeId};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ValidationResult {
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
}

impl ValidationResult {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationError {
    DuplicateId,
    DanglingDependency,
    LayerViolation,
    DependencyCycle,
    MissingInterface,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationWarning {
    DomainViolation,
    UnlayeredComponent,
}

pub fn validate_ir(ir: &ArchitectureIR) -> ValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    if has_duplicate_ids(ir) {
        errors.push(ValidationError::DuplicateId);
    }

    let nodes = ir
        .domains
        .iter()
        .map(|domain| NodeId::Domain(domain.id))
        .chain(
            ir.components
                .iter()
                .map(|component| NodeId::Component(component.id)),
        )
        .chain(
            ir.structures
                .iter()
                .map(|structure| NodeId::Structure(structure.id)),
        )
        .collect::<BTreeSet<_>>();
    if ir
        .dependencies
        .iter()
        .any(|edge| !nodes.contains(&edge.source) || !nodes.contains(&edge.target))
    {
        errors.push(ValidationError::DanglingDependency);
    }

    if has_layer_violation(ir) {
        errors.push(ValidationError::LayerViolation);
    }

    if is_cyclic_directed(&ir.to_graph()) {
        errors.push(ValidationError::DependencyCycle);
    }

    if has_missing_interface(ir) {
        errors.push(ValidationError::MissingInterface);
    }

    if has_domain_violation(ir) {
        warnings.push(ValidationWarning::DomainViolation);
    }

    if has_unlayered_component(ir) {
        warnings.push(ValidationWarning::UnlayeredComponent);
    }

    ValidationResult { errors, warnings }
}

fn has_duplicate_ids(ir: &ArchitectureIR) -> bool {
    let mut ids = BTreeSet::new();
    ir.domains
        .iter()
        .map(|domain| domain.id)
        .chain(ir.components.iter().map(|component| component.id))
        .chain(ir.interfaces.iter().map(|interface| interface.id))
        .chain(ir.structures.iter().map(|structure| structure.id))
        .chain(ir.design_units.iter().map(|design_unit| design_unit.id))
        .any(|id| !ids.insert(id))
}

fn has_layer_violation(ir: &ArchitectureIR) -> bool {
    let component_types = ir
        .components
        .iter()
        .map(|component| (component.id, &component.component_type))
        .collect::<BTreeMap<_, _>>();
    let component_layers = ir
        .layers
        .iter()
        .flat_map(|layer| {
            layer
                .components
                .iter()
                .map(move |component| (*component, layer))
        })
        .collect::<BTreeMap<_, _>>();

    ir.dependencies
        .iter()
        .any(|edge| match (edge.source, edge.target) {
            (NodeId::Component(source), NodeId::Component(target)) => {
                let Some(source_layer) = component_layers.get(&source) else {
                    return false;
                };
                let Some(target_layer) = component_layers.get(&target) else {
                    return false;
                };
                let Some(source_type) = component_types.get(&source) else {
                    return false;
                };
                let Some(target_type) = component_types.get(&target) else {
                    return false;
                };

                if source_layer.level < target_layer.level {
                    return true;
                }

                !dependency_allowed(
                    source_layer.allowed_dependencies.as_slice(),
                    source_type,
                    target_type,
                ) || !dependency_allowed(
                    target_layer.allowed_dependencies.as_slice(),
                    source_type,
                    target_type,
                ) && source_layer.name == target_layer.name
            }
            _ => false,
        })
}

fn dependency_allowed(
    rules: &[crate::LayerRule],
    from: &ComponentType,
    to: &ComponentType,
) -> bool {
    if rules.is_empty() {
        return true;
    }
    rules
        .iter()
        .any(|rule| &rule.from == from && &rule.to == to)
}

fn has_domain_violation(ir: &ArchitectureIR) -> bool {
    let known_components = ir
        .components
        .iter()
        .map(|component| component.id)
        .collect::<BTreeSet<_>>();
    let mut assigned = BTreeSet::new();

    for domain in &ir.domains {
        for component in &domain.components {
            if !known_components.contains(component) || !assigned.insert(*component) {
                return true;
            }
        }
    }

    false
}

fn has_unlayered_component(ir: &ArchitectureIR) -> bool {
    let layered = ir
        .layers
        .iter()
        .flat_map(|layer| layer.components.iter().copied())
        .collect::<BTreeSet<_>>();
    ir.components
        .iter()
        .any(|component| !layered.contains(&component.id))
}

fn has_missing_interface(ir: &ArchitectureIR) -> bool {
    let interface_ids = ir
        .interfaces
        .iter()
        .map(|interface| interface.id)
        .collect::<BTreeSet<_>>();
    let component_ids = ir
        .components
        .iter()
        .map(|component| component.id)
        .collect::<BTreeSet<_>>();

    if ir
        .interfaces
        .iter()
        .any(|interface| !component_ids.contains(&interface.owner_component))
    {
        return true;
    }

    ir.components.iter().any(|component| {
        component
            .interfaces
            .iter()
            .any(|interface_id| !interface_ids.contains(interface_id))
    }) || ir.dependencies.iter().any(|edge| {
        edge.interface
            .map(|interface_id| !interface_ids.contains(&interface_id))
            .unwrap_or(false)
    })
}
