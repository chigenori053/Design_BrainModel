use design_domain::{Architecture, Constraint, Dependency, Layer};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct ComponentId(pub u64);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum ComponentRole {
    Controller,
    Service,
    Repository,
    Database,
    Gateway,
    Unknown(String),
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct Interface {
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Component {
    pub id: ComponentId,
    pub role: ComponentRole,
    pub inputs: Vec<Interface>,
    pub outputs: Vec<Interface>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DeploymentModel {
    pub topology: String,
    pub replicas: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArchitectureMetrics {
    pub component_count: usize,
    pub dependency_count: usize,
    pub layering_score: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArchitectureState {
    pub components: Vec<Component>,
    pub dependencies: Vec<Dependency>,
    pub deployment: DeploymentModel,
    pub constraints: Vec<Constraint>,
    pub metrics: ArchitectureMetrics,
}

impl ArchitectureState {
    pub fn from_architecture(architecture: &Architecture, constraints: Vec<Constraint>) -> Self {
        let components = architecture
            .design_units_by_id()
            .values()
            .map(|unit| Component {
                id: ComponentId(unit.id.0),
                role: role_from_layer(unit.layer, &unit.name),
                inputs: vec![Interface {
                    name: format!("{}_in", unit.name.to_ascii_lowercase()),
                }],
                outputs: vec![Interface {
                    name: format!("{}_out", unit.name.to_ascii_lowercase()),
                }],
            })
            .collect::<Vec<_>>();
        let dependency_count = architecture.dependencies.len();
        let component_count = components.len();
        let layered_pairs = architecture
            .dependencies
            .iter()
            .filter(|dependency| dependency.from.0 <= dependency.to.0)
            .count();
        let layering_score = if dependency_count == 0 {
            1.0
        } else {
            layered_pairs as f64 / dependency_count as f64
        };
        Self {
            components,
            dependencies: architecture.dependencies.clone(),
            deployment: DeploymentModel {
                topology: if component_count > 4 {
                    "distributed".to_string()
                } else {
                    "monolith".to_string()
                },
                replicas: component_count.max(1),
            },
            constraints,
            metrics: ArchitectureMetrics {
                component_count,
                dependency_count,
                layering_score,
            },
        }
    }
}

impl Default for DeploymentModel {
    fn default() -> Self {
        Self {
            topology: "monolith".to_string(),
            replicas: 1,
        }
    }
}

impl Default for ArchitectureState {
    fn default() -> Self {
        Self {
            components: Vec::new(),
            dependencies: Vec::new(),
            deployment: DeploymentModel::default(),
            constraints: Vec::new(),
            metrics: ArchitectureMetrics::default(),
        }
    }
}

fn role_from_layer(layer: Layer, name: &str) -> ComponentRole {
    match layer {
        Layer::Ui => ComponentRole::Controller,
        Layer::Service => {
            if name.to_ascii_lowercase().contains("gateway") {
                ComponentRole::Gateway
            } else {
                ComponentRole::Service
            }
        }
        Layer::Repository => ComponentRole::Repository,
        Layer::Database => ComponentRole::Database,
    }
}
