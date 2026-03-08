use design_domain::{Architecture, Constraint};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LayerConstraint {
    pub require_layering: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyConstraint {
    pub no_cyclic_dependency: bool,
    pub max_dependencies: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ComplexityConstraint {
    pub max_class_units: usize,
    pub max_dependencies: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamingConstraint {
    pub require_pascal_case: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstraintRule {
    pub layer: LayerConstraint,
    pub dependency: DependencyConstraint,
    pub complexity: ComplexityConstraint,
    pub naming: NamingConstraint,
}

impl Default for ConstraintRule {
    fn default() -> Self {
        Self {
            layer: LayerConstraint {
                require_layering: true,
            },
            dependency: DependencyConstraint {
                no_cyclic_dependency: true,
                max_dependencies: 200,
            },
            complexity: ComplexityConstraint {
                max_class_units: 50,
                max_dependencies: 200,
            },
            naming: NamingConstraint {
                require_pascal_case: true,
            },
        }
    }
}

pub fn validate_constraint_rules(
    architecture: &Architecture,
    constraints: &[Constraint],
    rule: &ConstraintRule,
) -> Vec<String> {
    let mut issues = Vec::new();

    if architecture.classes.len() > rule.complexity.max_class_units {
        issues.push("class unit limit exceeded".to_string());
    }
    if architecture.dependencies.len() > rule.complexity.max_dependencies {
        issues.push("dependency limit exceeded".to_string());
    }

    if rule.naming.require_pascal_case {
        for class_unit in &architecture.classes {
            if !starts_with_uppercase(&class_unit.name) {
                issues.push(format!("class '{}' violates naming rule", class_unit.name));
            }
            for structure in &class_unit.structures {
                if structure.name.trim().is_empty() {
                    issues.push("structure name must not be empty".to_string());
                }
                for unit in &structure.design_units {
                    if unit.name.trim().is_empty() {
                        issues.push("design unit name must not be empty".to_string());
                    }
                }
            }
        }
    }

    for constraint in constraints {
        if !constraint.satisfied_by(architecture) {
            issues.push(format!("constraint '{}' rejected architecture", constraint.name));
        }
    }

    issues
}

fn starts_with_uppercase(value: &str) -> bool {
    value.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false)
}
