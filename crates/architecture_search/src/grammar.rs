use architecture_ir::ComponentType;

use crate::grammar_engine::{default_component_rule, default_interface_rule};
use crate::intent::{IntentModel, normalize_key};
use crate::search_space::DependencyRule;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchitectureStyle {
    Layered,
    Pipeline,
    Generic,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentRule {
    pub name: String,
    pub component_type: ComponentType,
    pub layer: String,
    pub allowed_dependencies: Vec<ComponentType>,
    pub required_interfaces: Vec<ComponentType>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerRule {
    pub name: String,
    pub level: usize,
    pub allowed_targets: Vec<String>,
    pub contained_components: Vec<ComponentType>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InterfaceRule {
    pub exposer: ComponentType,
    pub interface_type: ComponentType,
    pub implementors: Vec<ComponentType>,
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ConstraintRule {
    pub max_dependencies_per_component: usize,
    pub no_circular_dependency: bool,
    pub max_layer_depth: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchitectureGrammar {
    pub style: ArchitectureStyle,
    pub component_rules: Vec<ComponentRule>,
    pub dependency_rules: Vec<DependencyRule>,
    pub layer_rules: Vec<LayerRule>,
    pub interface_rules: Vec<InterfaceRule>,
    pub constraint_rule: ConstraintRule,
}

impl ArchitectureGrammar {
    pub fn from_intent(intent: &IntentModel) -> Self {
        match resolve_style(intent) {
            ArchitectureStyle::Layered => Self::layered(),
            ArchitectureStyle::Pipeline => Self::pipeline(),
            ArchitectureStyle::Generic => Self::generic(),
        }
    }

    pub fn from_dsl(input: &str) -> Result<Self, String> {
        let mut component_rules = Vec::new();
        let mut layer_rules = Vec::new();
        let mut dependency_rules = Vec::new();
        let mut current_component: Option<usize> = None;
        let mut current_layer: Option<usize> = None;

        for raw in input.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix("component ") {
                component_rules.push(ComponentRule {
                    name: name.trim().to_string(),
                    component_type: parse_component_type(name.trim())?,
                    layer: "Application".to_string(),
                    allowed_dependencies: Vec::new(),
                    required_interfaces: Vec::new(),
                });
                current_component = Some(component_rules.len() - 1);
                current_layer = None;
                continue;
            }
            if let Some(name) = line.strip_prefix("layer ") {
                layer_rules.push(LayerRule {
                    name: name.trim().to_string(),
                    level: layer_rules.len() + 1,
                    allowed_targets: Vec::new(),
                    contained_components: Vec::new(),
                });
                current_layer = Some(layer_rules.len() - 1);
                current_component = None;
                continue;
            }
            if let Some(name) = line.strip_prefix("depends_on ") {
                let component_type = parse_component_type(name.trim())?;
                let Some(index) = current_component else {
                    return Err("depends_on must appear after component".to_string());
                };
                component_rules[index]
                    .allowed_dependencies
                    .push(component_type.clone());
                dependency_rules.push(DependencyRule {
                    from: component_rules[index].component_type.clone(),
                    to: component_type,
                });
                continue;
            }
            if let Some(name) = line.strip_prefix("requires_interface ") {
                let interface_type = parse_component_type(name.trim())?;
                let Some(index) = current_component else {
                    return Err("requires_interface must appear after component".to_string());
                };
                component_rules[index]
                    .required_interfaces
                    .push(interface_type);
                continue;
            }
            if let Some(name) = line.strip_prefix("contains ") {
                let component_type = parse_component_type(name.trim())?;
                let Some(index) = current_layer else {
                    return Err("contains must appear after layer".to_string());
                };
                layer_rules[index]
                    .contained_components
                    .push(component_type.clone());
                if let Some(component) = component_rules
                    .iter_mut()
                    .find(|rule| rule.component_type == component_type)
                {
                    component.layer = layer_rules[index].name.clone();
                }
                continue;
            }
            if let Some(name) = line.strip_prefix("allows ") {
                let Some(index) = current_layer else {
                    return Err("allows must appear after layer".to_string());
                };
                layer_rules[index]
                    .allowed_targets
                    .push(name.trim().to_string());
                continue;
            }
            return Err(format!("unrecognized grammar dsl line: {line}"));
        }

        Ok(Self {
            style: ArchitectureStyle::Generic,
            component_rules,
            dependency_rules,
            layer_rules,
            interface_rules: Vec::new(),
            constraint_rule: ConstraintRule {
                max_dependencies_per_component: 5,
                no_circular_dependency: true,
                max_layer_depth: 4,
            },
        })
    }

    pub fn component_catalog(&self) -> Vec<ComponentType> {
        let mut component_types = self
            .component_rules
            .iter()
            .map(|rule| rule.component_type.clone())
            .collect::<Vec<_>>();
        component_types.sort_by_key(component_sort_key);
        component_types.dedup();
        component_types
    }

    pub fn dependency_rules(&self) -> Vec<DependencyRule> {
        self.dependency_rules.clone()
    }

    fn layered() -> Self {
        let component_rules = vec![
            default_component_rule(
                "Controller",
                ComponentType::Controller,
                "Presentation",
                vec![ComponentType::Service, ComponentType::UseCase],
            ),
            default_component_rule(
                "Service",
                ComponentType::Service,
                "Application",
                vec![
                    ComponentType::Repository,
                    ComponentType::Adapter,
                    ComponentType::DomainModel,
                    ComponentType::Interface,
                ],
            ),
            default_component_rule(
                "UseCase",
                ComponentType::UseCase,
                "Application",
                vec![
                    ComponentType::Repository,
                    ComponentType::Adapter,
                    ComponentType::DomainModel,
                    ComponentType::Interface,
                ],
            ),
            default_component_rule("DomainModel", ComponentType::DomainModel, "Domain", vec![]),
            default_component_rule(
                "Repository",
                ComponentType::Repository,
                "Infrastructure",
                vec![ComponentType::DataModel, ComponentType::Interface],
            ),
            default_component_rule(
                "Adapter",
                ComponentType::Adapter,
                "Infrastructure",
                vec![ComponentType::Repository, ComponentType::DataModel],
            ),
            default_component_rule("Interface", ComponentType::Interface, "Domain", vec![]),
            default_component_rule(
                "Database",
                ComponentType::DataModel,
                "Infrastructure",
                vec![],
            ),
        ];
        let dependency_rules = vec![
            rule(ComponentType::Controller, ComponentType::Service),
            rule(ComponentType::Controller, ComponentType::UseCase),
            rule(ComponentType::Service, ComponentType::Repository),
            rule(ComponentType::Service, ComponentType::Adapter),
            rule(ComponentType::Service, ComponentType::DomainModel),
            rule(ComponentType::Service, ComponentType::Interface),
            rule(ComponentType::UseCase, ComponentType::Repository),
            rule(ComponentType::UseCase, ComponentType::Adapter),
            rule(ComponentType::UseCase, ComponentType::DomainModel),
            rule(ComponentType::UseCase, ComponentType::Interface),
            rule(ComponentType::Repository, ComponentType::DataModel),
            rule(ComponentType::Repository, ComponentType::Interface),
            rule(ComponentType::Adapter, ComponentType::Repository),
            rule(ComponentType::Adapter, ComponentType::DataModel),
        ];
        let layer_rules = vec![
            layer(
                "Presentation",
                4,
                vec!["Application"],
                vec![ComponentType::Controller],
            ),
            layer(
                "Application",
                3,
                vec!["Domain", "Infrastructure"],
                vec![ComponentType::Service, ComponentType::UseCase],
            ),
            layer(
                "Domain",
                2,
                vec!["Infrastructure"],
                vec![ComponentType::DomainModel, ComponentType::Interface],
            ),
            layer(
                "Infrastructure",
                1,
                vec![],
                vec![
                    ComponentType::Repository,
                    ComponentType::Adapter,
                    ComponentType::DataModel,
                ],
            ),
        ];
        let interface_rules = vec![default_interface_rule(
            ComponentType::Service,
            ComponentType::Interface,
            vec![ComponentType::Repository],
            false,
        )];
        Self {
            style: ArchitectureStyle::Layered,
            component_rules,
            dependency_rules,
            layer_rules,
            interface_rules,
            constraint_rule: ConstraintRule {
                max_dependencies_per_component: 5,
                no_circular_dependency: true,
                max_layer_depth: 4,
            },
        }
    }

    fn pipeline() -> Self {
        let component_rules = vec![
            default_component_rule(
                "Gateway",
                ComponentType::Controller,
                "Presentation",
                vec![ComponentType::Service],
            ),
            default_component_rule(
                "Process",
                ComponentType::Service,
                "Application",
                vec![ComponentType::Adapter, ComponentType::Repository],
            ),
            default_component_rule(
                "Queue",
                ComponentType::Adapter,
                "Infrastructure",
                vec![ComponentType::Repository],
            ),
            default_component_rule(
                "Store",
                ComponentType::Repository,
                "Infrastructure",
                vec![ComponentType::DataModel],
            ),
            default_component_rule(
                "Database",
                ComponentType::DataModel,
                "Infrastructure",
                vec![],
            ),
        ];
        let dependency_rules = vec![
            rule(ComponentType::Controller, ComponentType::Service),
            rule(ComponentType::Service, ComponentType::Adapter),
            rule(ComponentType::Service, ComponentType::Repository),
            rule(ComponentType::Adapter, ComponentType::Repository),
            rule(ComponentType::Repository, ComponentType::DataModel),
        ];
        let layer_rules = vec![
            layer(
                "Presentation",
                3,
                vec!["Application"],
                vec![ComponentType::Controller],
            ),
            layer(
                "Application",
                2,
                vec!["Infrastructure"],
                vec![ComponentType::Service],
            ),
            layer(
                "Infrastructure",
                1,
                vec![],
                vec![
                    ComponentType::Adapter,
                    ComponentType::Repository,
                    ComponentType::DataModel,
                ],
            ),
        ];
        Self {
            style: ArchitectureStyle::Pipeline,
            component_rules,
            dependency_rules,
            layer_rules,
            interface_rules: Vec::new(),
            constraint_rule: ConstraintRule {
                max_dependencies_per_component: 4,
                no_circular_dependency: true,
                max_layer_depth: 3,
            },
        }
    }

    fn generic() -> Self {
        let component_rules = vec![
            default_component_rule(
                "Service",
                ComponentType::Service,
                "Application",
                vec![ComponentType::Repository, ComponentType::Adapter],
            ),
            default_component_rule(
                "Repository",
                ComponentType::Repository,
                "Infrastructure",
                vec![ComponentType::DataModel],
            ),
            default_component_rule(
                "Adapter",
                ComponentType::Adapter,
                "Infrastructure",
                vec![ComponentType::Repository],
            ),
            default_component_rule(
                "Database",
                ComponentType::DataModel,
                "Infrastructure",
                vec![],
            ),
        ];
        let dependency_rules = vec![
            rule(ComponentType::Service, ComponentType::Repository),
            rule(ComponentType::Service, ComponentType::Adapter),
            rule(ComponentType::Repository, ComponentType::DataModel),
            rule(ComponentType::Adapter, ComponentType::Repository),
        ];
        let layer_rules = vec![
            layer(
                "Application",
                2,
                vec!["Infrastructure"],
                vec![ComponentType::Service],
            ),
            layer(
                "Infrastructure",
                1,
                vec![],
                vec![
                    ComponentType::Repository,
                    ComponentType::Adapter,
                    ComponentType::DataModel,
                ],
            ),
        ];
        Self {
            style: ArchitectureStyle::Generic,
            component_rules,
            dependency_rules,
            layer_rules,
            interface_rules: Vec::new(),
            constraint_rule: ConstraintRule {
                max_dependencies_per_component: 5,
                no_circular_dependency: true,
                max_layer_depth: 4,
            },
        }
    }
}

fn resolve_style(intent: &IntentModel) -> ArchitectureStyle {
    intent
        .constraints
        .architecture
        .as_deref()
        .map(normalize_key)
        .map(|value| match value.as_str() {
            "layered" => ArchitectureStyle::Layered,
            "pipeline" | "eventdriven" | "event_driven" => ArchitectureStyle::Pipeline,
            _ => ArchitectureStyle::Generic,
        })
        .unwrap_or_else(|| match normalize_key(&intent.system_type).as_str() {
            "webapi" | "web_api" | "api" => ArchitectureStyle::Layered,
            "pipeline" | "datapipeline" | "data_pipeline" => ArchitectureStyle::Pipeline,
            _ => ArchitectureStyle::Generic,
        })
}

fn rule(from: ComponentType, to: ComponentType) -> DependencyRule {
    DependencyRule { from, to }
}

fn layer(
    name: &str,
    level: usize,
    allowed_targets: Vec<&str>,
    contained_components: Vec<ComponentType>,
) -> LayerRule {
    LayerRule {
        name: name.to_string(),
        level,
        allowed_targets: allowed_targets.into_iter().map(str::to_string).collect(),
        contained_components,
    }
}

fn parse_component_type(value: &str) -> Result<ComponentType, String> {
    match normalize_key(value).as_str() {
        "controller" | "gateway" => Ok(ComponentType::Controller),
        "service" | "process" => Ok(ComponentType::Service),
        "repository" | "store" => Ok(ComponentType::Repository),
        "adapter" | "queue" | "cache" => Ok(ComponentType::Adapter),
        "interface" => Ok(ComponentType::Interface),
        "domainmodel" | "domain_model" => Ok(ComponentType::DomainModel),
        "usecase" | "use_case" => Ok(ComponentType::UseCase),
        "datamodel" | "data_model" | "database" => Ok(ComponentType::DataModel),
        other => Err(format!("unknown component type '{other}'")),
    }
}

fn component_sort_key(component_type: &ComponentType) -> usize {
    match component_type {
        ComponentType::Controller => 0,
        ComponentType::Service => 1,
        ComponentType::UseCase => 2,
        ComponentType::Interface => 3,
        ComponentType::DomainModel => 4,
        ComponentType::Adapter => 5,
        ComponentType::Repository => 6,
        ComponentType::DataModel => 7,
        ComponentType::Module => 8,
        ComponentType::Package => 9,
        ComponentType::Class => 10,
        ComponentType::Struct => 11,
        ComponentType::Trait => 12,
        ComponentType::Function => 13,
        ComponentType::Method => 14,
    }
}
