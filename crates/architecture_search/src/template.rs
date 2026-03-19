use architecture_ir::{ArchitectureConstraint, ComponentType, ConstraintType, ConstraintValue};

use crate::DependencyRule;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchitectureTemplate {
    pub template_id: String,
    pub topology: Topology,
    pub layer_structure: Vec<TemplateLayer>,
    pub component_slots: Vec<ComponentSlot>,
    pub dependency_rules: Vec<DependencyRule>,
    pub constraints: Vec<ArchitectureConstraint>,
    pub ranking: TemplateRanking,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Topology {
    Layered,
    Hexagonal,
    Microservice,
    EventDriven,
    Pipeline,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateLayer {
    pub name: String,
    pub level: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComponentSlot {
    pub layer: String,
    pub slot_name: String,
    pub slot_type: ComponentType,
    pub optional: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateRanking {
    pub historical_success: u32,
    pub pattern_stability: u32,
    pub complexity: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateSelection {
    pub selected: ArchitectureTemplate,
    pub alternatives: Vec<ArchitectureTemplate>,
}

pub fn builtin_templates() -> Vec<ArchitectureTemplate> {
    vec![
        layered_template(),
        hexagonal_template(),
        pipeline_template(),
        event_driven_template(),
        microservice_template(),
    ]
}

fn layered_template() -> ArchitectureTemplate {
    ArchitectureTemplate {
        template_id: "layered".to_string(),
        topology: Topology::Layered,
        layer_structure: vec![
            TemplateLayer {
                name: "Presentation".to_string(),
                level: 4,
            },
            TemplateLayer {
                name: "Application".to_string(),
                level: 3,
            },
            TemplateLayer {
                name: "Domain".to_string(),
                level: 2,
            },
            TemplateLayer {
                name: "Infrastructure".to_string(),
                level: 1,
            },
        ],
        component_slots: vec![
            slot(
                "Presentation",
                "ApiController",
                ComponentType::Controller,
                false,
            ),
            slot("Application", "Service", ComponentType::Service, false),
            slot("Domain", "DomainModel", ComponentType::DomainModel, true),
            slot(
                "Infrastructure",
                "Repository",
                ComponentType::Repository,
                false,
            ),
            slot(
                "Infrastructure",
                "CacheAdapter",
                ComponentType::Adapter,
                true,
            ),
        ],
        dependency_rules: vec![
            dep(ComponentType::Controller, ComponentType::Service),
            dep(ComponentType::Service, ComponentType::Repository),
            dep(ComponentType::Service, ComponentType::DomainModel),
            dep(ComponentType::Service, ComponentType::Adapter),
            dep(ComponentType::Repository, ComponentType::DataModel),
        ],
        constraints: vec![
            ArchitectureConstraint {
                constraint_type: ConstraintType::NoCircularDependency,
                description: "template requires acyclic layering".to_string(),
                value: Some(ConstraintValue::Boolean(true)),
            },
            ArchitectureConstraint {
                constraint_type: ConstraintType::LayerViolation,
                description: "presentation depends inward only".to_string(),
                value: Some(ConstraintValue::Boolean(true)),
            },
        ],
        ranking: TemplateRanking {
            historical_success: 9,
            pattern_stability: 9,
            complexity: 4,
        },
    }
}

fn hexagonal_template() -> ArchitectureTemplate {
    ArchitectureTemplate {
        template_id: "hexagonal".to_string(),
        topology: Topology::Hexagonal,
        layer_structure: vec![
            TemplateLayer {
                name: "Adapters".to_string(),
                level: 3,
            },
            TemplateLayer {
                name: "Ports".to_string(),
                level: 2,
            },
            TemplateLayer {
                name: "Domain".to_string(),
                level: 1,
            },
        ],
        component_slots: vec![
            slot("Adapters", "InboundAdapter", ComponentType::Adapter, false),
            slot("Ports", "Port", ComponentType::Interface, false),
            slot("Domain", "DomainService", ComponentType::Service, false),
            slot(
                "Adapters",
                "OutboundAdapter",
                ComponentType::Repository,
                true,
            ),
        ],
        dependency_rules: vec![
            dep(ComponentType::Adapter, ComponentType::Interface),
            dep(ComponentType::Service, ComponentType::Interface),
            dep(ComponentType::Repository, ComponentType::DataModel),
        ],
        constraints: vec![ArchitectureConstraint {
            constraint_type: ConstraintType::LayerViolation,
            description: "adapters must go through ports".to_string(),
            value: Some(ConstraintValue::Boolean(true)),
        }],
        ranking: TemplateRanking {
            historical_success: 8,
            pattern_stability: 8,
            complexity: 6,
        },
    }
}

fn pipeline_template() -> ArchitectureTemplate {
    ArchitectureTemplate {
        template_id: "pipeline".to_string(),
        topology: Topology::Pipeline,
        layer_structure: vec![
            TemplateLayer {
                name: "Ingest".to_string(),
                level: 3,
            },
            TemplateLayer {
                name: "Process".to_string(),
                level: 2,
            },
            TemplateLayer {
                name: "Store".to_string(),
                level: 1,
            },
        ],
        component_slots: vec![
            slot("Ingest", "Gateway", ComponentType::Controller, false),
            slot("Process", "Processor", ComponentType::Service, false),
            slot("Store", "Repository", ComponentType::Repository, false),
        ],
        dependency_rules: vec![
            dep(ComponentType::Controller, ComponentType::Service),
            dep(ComponentType::Service, ComponentType::Repository),
        ],
        constraints: vec![ArchitectureConstraint {
            constraint_type: ConstraintType::DependencyLimit,
            description: "pipeline stages remain sparse".to_string(),
            value: Some(ConstraintValue::Integer(4)),
        }],
        ranking: TemplateRanking {
            historical_success: 8,
            pattern_stability: 7,
            complexity: 4,
        },
    }
}

fn event_driven_template() -> ArchitectureTemplate {
    ArchitectureTemplate {
        template_id: "event_driven".to_string(),
        topology: Topology::EventDriven,
        layer_structure: vec![
            TemplateLayer {
                name: "Ingress".to_string(),
                level: 3,
            },
            TemplateLayer {
                name: "Handlers".to_string(),
                level: 2,
            },
            TemplateLayer {
                name: "Persistence".to_string(),
                level: 1,
            },
        ],
        component_slots: vec![
            slot("Ingress", "EventGateway", ComponentType::Controller, false),
            slot("Handlers", "Handler", ComponentType::Service, false),
            slot(
                "Persistence",
                "EventStore",
                ComponentType::Repository,
                false,
            ),
            slot("Persistence", "Queue", ComponentType::Adapter, true),
        ],
        dependency_rules: vec![
            dep(ComponentType::Controller, ComponentType::Adapter),
            dep(ComponentType::Adapter, ComponentType::Service),
            dep(ComponentType::Service, ComponentType::Repository),
        ],
        constraints: vec![ArchitectureConstraint {
            constraint_type: ConstraintType::NoCircularDependency,
            description: "event chain must remain acyclic".to_string(),
            value: Some(ConstraintValue::Boolean(true)),
        }],
        ranking: TemplateRanking {
            historical_success: 7,
            pattern_stability: 7,
            complexity: 6,
        },
    }
}

fn microservice_template() -> ArchitectureTemplate {
    ArchitectureTemplate {
        template_id: "microservice".to_string(),
        topology: Topology::Microservice,
        layer_structure: vec![
            TemplateLayer {
                name: "Gateway".to_string(),
                level: 3,
            },
            TemplateLayer {
                name: "Services".to_string(),
                level: 2,
            },
            TemplateLayer {
                name: "Data".to_string(),
                level: 1,
            },
        ],
        component_slots: vec![
            slot("Gateway", "ApiGateway", ComponentType::Controller, false),
            slot("Services", "ServiceA", ComponentType::Service, false),
            slot("Services", "ServiceB", ComponentType::Service, true),
            slot(
                "Data",
                "ServiceRepository",
                ComponentType::Repository,
                false,
            ),
        ],
        dependency_rules: vec![
            dep(ComponentType::Controller, ComponentType::Service),
            dep(ComponentType::Service, ComponentType::Repository),
        ],
        constraints: vec![ArchitectureConstraint {
            constraint_type: ConstraintType::DependencyLimit,
            description: "service fanout bounded".to_string(),
            value: Some(ConstraintValue::Integer(5)),
        }],
        ranking: TemplateRanking {
            historical_success: 6,
            pattern_stability: 6,
            complexity: 8,
        },
    }
}

fn slot(layer: &str, slot_name: &str, slot_type: ComponentType, optional: bool) -> ComponentSlot {
    ComponentSlot {
        layer: layer.to_string(),
        slot_name: slot_name.to_string(),
        slot_type,
        optional,
    }
}

fn dep(from: ComponentType, to: ComponentType) -> DependencyRule {
    DependencyRule { from, to }
}
