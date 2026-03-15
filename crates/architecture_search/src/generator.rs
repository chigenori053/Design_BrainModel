use architecture_ir::{
    ArchitectureIR, ComponentMetrics, ComponentType, ComponentUnit, DependencyEdge, DependencyType,
    Layer, NodeId, Visibility,
};

use crate::{SearchSpace, SearchState, search_space::DesignIntent};

pub trait CandidateGenerator {
    fn generate(&self, state: &SearchState) -> Vec<SearchState>;
}

#[derive(Clone, Debug)]
pub struct DeterministicCandidateGenerator {
    search_space: SearchSpace,
    intent: DesignIntent,
}

impl DeterministicCandidateGenerator {
    pub fn new(search_space: SearchSpace, intent: DesignIntent) -> Self {
        Self {
            search_space,
            intent,
        }
    }
}

impl CandidateGenerator for DeterministicCandidateGenerator {
    fn generate(&self, state: &SearchState) -> Vec<SearchState> {
        let mut candidates = Vec::new();
        candidates.extend(self.generate_component_candidates(state));
        candidates.extend(self.generate_dependency_candidates(state));

        for (index, candidate) in candidates.iter_mut().enumerate() {
            candidate.state_id = state
                .state_id
                .saturating_mul(1_000)
                .saturating_add(index as u64 + 1);
        }

        candidates
    }
}

impl DeterministicCandidateGenerator {
    fn generate_component_candidates(&self, state: &SearchState) -> Vec<SearchState> {
        let existing_types = state
            .architecture
            .components
            .iter()
            .map(|component| component.component_type.clone())
            .collect::<Vec<_>>();

        let mut target_types = self
            .intent
            .required_components
            .iter()
            .filter(|component_type| !existing_types.contains(component_type))
            .cloned()
            .collect::<Vec<_>>();

        if target_types.is_empty() {
            target_types = self
                .search_space
                .component_catalog
                .iter()
                .filter(|component_type| !existing_types.contains(component_type))
                .cloned()
                .collect::<Vec<_>>();
        }

        target_types.sort_by_key(component_type_key);
        target_types
            .into_iter()
            .map(|component_type| {
                let mut next = state.clone();
                next.depth = next.depth.saturating_add(1);
                add_component(&mut next.architecture, component_type);
                next
            })
            .collect()
    }

    fn generate_dependency_candidates(&self, state: &SearchState) -> Vec<SearchState> {
        let mut rules = self.search_space.allowed_dependencies.clone();
        rules.sort_by_key(|rule| {
            (
                component_type_key(&rule.from),
                component_type_key(&rule.to),
                component_type_label(&rule.from),
                component_type_label(&rule.to),
            )
        });

        let components = state.architecture.components.clone();
        let existing_edges = state
            .architecture
            .dependencies
            .iter()
            .map(|edge| (edge.source, edge.target))
            .collect::<Vec<_>>();

        let mut candidates = Vec::new();
        for rule in rules {
            for from in components
                .iter()
                .filter(|component| component.component_type == rule.from)
            {
                for to in components
                    .iter()
                    .filter(|component| component.component_type == rule.to)
                {
                    if from.id == to.id {
                        continue;
                    }
                    let edge = (NodeId::Component(from.id), NodeId::Component(to.id));
                    if existing_edges.contains(&edge) {
                        continue;
                    }

                    let mut next = state.clone();
                    next.depth = next.depth.saturating_add(1);
                    next.architecture.dependencies.push(DependencyEdge {
                        source: edge.0,
                        target: edge.1,
                        dependency_type: DependencyType::Use,
                    });
                    candidates.push(next);
                }
            }
        }

        candidates
    }
}

fn add_component(ir: &mut ArchitectureIR, component_type: ComponentType) {
    let next_component_id = ir
        .components
        .iter()
        .map(|component| component.id)
        .max()
        .unwrap_or(0)
        .saturating_add(1);

    let component = ComponentUnit {
        id: next_component_id,
        name: format!(
            "{}{}",
            component_type_label(&component_type),
            component_type_count(ir, &component_type) + 1
        ),
        component_type: component_type.clone(),
        structures: Vec::new(),
        visibility: Visibility::Public,
        metrics: ComponentMetrics::default(),
    };
    ir.components.push(component);
    upsert_layer(ir, component_type, next_component_id);
}

fn upsert_layer(ir: &mut ArchitectureIR, component_type: ComponentType, component_id: u64) {
    let (name, level) = default_layer(&component_type);
    if let Some(layer) = ir.layers.iter_mut().find(|layer| layer.name == name) {
        layer.components.push(component_id);
        layer.components.sort_unstable();
        return;
    }

    ir.layers.push(Layer {
        name: name.to_string(),
        level,
        components: vec![component_id],
        allowed_dependencies: Vec::new(),
    });
    ir.layers.sort_by(|lhs, rhs| {
        rhs.level
            .cmp(&lhs.level)
            .then_with(|| lhs.name.cmp(&rhs.name))
    });
}

fn component_type_count(ir: &ArchitectureIR, component_type: &ComponentType) -> usize {
    ir.components
        .iter()
        .filter(|component| &component.component_type == component_type)
        .count()
}

fn default_layer(component_type: &ComponentType) -> (&'static str, u32) {
    match component_type {
        ComponentType::Controller => ("Presentation", 3),
        ComponentType::Service | ComponentType::UseCase | ComponentType::DomainModel => {
            ("Application", 2)
        }
        ComponentType::Repository | ComponentType::Adapter => ("Infrastructure", 1),
        _ => ("Application", 2),
    }
}

pub(crate) fn component_type_key(component_type: &ComponentType) -> usize {
    match component_type {
        ComponentType::Controller => 0,
        ComponentType::Service => 1,
        ComponentType::UseCase => 2,
        ComponentType::Repository => 3,
        ComponentType::Adapter => 4,
        ComponentType::DomainModel => 5,
        ComponentType::DataModel => 6,
        ComponentType::Module => 7,
        ComponentType::Package => 8,
        ComponentType::Class => 9,
        ComponentType::Struct => 10,
        ComponentType::Trait => 11,
        ComponentType::Interface => 12,
        ComponentType::Function => 13,
        ComponentType::Method => 14,
    }
}

pub(crate) fn component_type_label(component_type: &ComponentType) -> &'static str {
    match component_type {
        ComponentType::Controller => "Controller",
        ComponentType::Service => "Service",
        ComponentType::UseCase => "UseCase",
        ComponentType::Repository => "Repository",
        ComponentType::Adapter => "Adapter",
        ComponentType::DomainModel => "DomainModel",
        ComponentType::DataModel => "DataModel",
        ComponentType::Module => "Module",
        ComponentType::Package => "Package",
        ComponentType::Class => "Class",
        ComponentType::Struct => "Struct",
        ComponentType::Trait => "Trait",
        ComponentType::Interface => "Interface",
        ComponentType::Function => "Function",
        ComponentType::Method => "Method",
    }
}
