use design_domain::Architecture;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArchitectureRule {
    ArchitectureMustHaveClass,
    ClassMustHaveStructure,
    StructureMustHaveDesignUnit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GrammarRule {
    ArchitectureRule(ArchitectureRule),
    ClassStructureRule,
    DependencyRule,
}

pub fn validate_architecture_rules(architecture: &Architecture) -> Vec<String> {
    let mut issues = Vec::new();

    if architecture.classes.is_empty() {
        issues.push("architecture must contain at least one class".to_string());
    }

    for class_unit in &architecture.classes {
        if class_unit.structures.is_empty() {
            issues.push(format!(
                "class '{}' must contain at least one structure",
                class_unit.name
            ));
        }

        for structure in &class_unit.structures {
            if structure.design_units.is_empty() {
                issues.push(format!(
                    "structure '{}' must contain at least one design unit",
                    structure.name
                ));
            }
        }
    }

    issues
}
