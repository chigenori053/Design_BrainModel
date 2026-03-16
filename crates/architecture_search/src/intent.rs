use architecture_ir::{ArchitectureConstraint, ComponentType, ConstraintType};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentModel {
    pub system_type: String,
    #[serde(default)]
    pub requirements: Vec<String>,
    #[serde(default)]
    pub constraints: IntentConstraints,
    #[serde(default)]
    pub quality_attributes: Vec<String>,
    #[serde(default, alias = "domain_knowledge")]
    pub domain_context: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentConstraints {
    pub architecture: Option<String>,
    pub language: Option<String>,
    #[serde(default)]
    pub forbidden_components: Vec<ComponentType>,
}

impl IntentModel {
    pub fn required_component_types(&self) -> Vec<ComponentType> {
        let mut components = match normalize_key(&self.system_type).as_str() {
            "webapi" | "web_api" | "api" => vec![
                ComponentType::Controller,
                ComponentType::Service,
                ComponentType::Repository,
            ],
            "eventdriven" | "event_driven" | "pipeline" | "datapipeline" | "data_pipeline" => {
                vec![
                    ComponentType::Controller,
                    ComponentType::Service,
                    ComponentType::Adapter,
                    ComponentType::Repository,
                ]
            }
            _ => vec![ComponentType::Service, ComponentType::Repository],
        };

        for requirement in &self.requirements {
            let normalized = normalize_key(requirement);
            let inferred = match normalized.as_str() {
                "authentication" | "auth" | "authorization" | "logging" | "log" => {
                    Some(ComponentType::Service)
                }
                "caching" | "cache" | "queue" | "messaging" => Some(ComponentType::Adapter),
                "persistence" | "storage" | "database" => Some(ComponentType::Repository),
                "workflow" | "application" | "orchestration" => Some(ComponentType::UseCase),
                "domainlogic" | "domain_logic" => Some(ComponentType::DomainModel),
                _ => None,
            };
            if let Some(component) = inferred {
                push_unique(&mut components, component);
            }
        }

        components.retain(|component| !self.constraints.forbidden_components.contains(component));
        components
    }

    pub fn architecture_constraints(&self) -> Vec<ArchitectureConstraint> {
        let mut constraints = vec![
            ArchitectureConstraint {
                constraint_type: ConstraintType::NoCircularDependency,
                description: "no cycles".to_string(),
                value: None,
            },
            ArchitectureConstraint {
                constraint_type: ConstraintType::LayerViolation,
                description: "respect architecture grammar".to_string(),
                value: None,
            },
        ];

        if self
            .quality_attributes
            .iter()
            .any(|item| normalize_key(item).contains("complex"))
        {
            constraints.push(ArchitectureConstraint {
                constraint_type: ConstraintType::ComplexityLimit,
                description: "limit complexity growth".to_string(),
                value: None,
            });
        }

        constraints
    }
}

fn push_unique(values: &mut Vec<ComponentType>, component: ComponentType) {
    if !values.contains(&component) {
        values.push(component);
    }
}

pub(crate) fn normalize_key(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .flat_map(|c| c.to_lowercase())
        .collect()
}
