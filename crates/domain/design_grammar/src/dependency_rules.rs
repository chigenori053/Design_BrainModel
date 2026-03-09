use design_domain::{Architecture, Layer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyRule {
    pub allowed: Vec<Layer>,
    pub forbidden: Vec<Layer>,
}

impl Default for DependencyRule {
    fn default() -> Self {
        Self {
            allowed: vec![
                Layer::Ui,
                Layer::Service,
                Layer::Repository,
                Layer::Database,
            ],
            forbidden: vec![
                Layer::Database,
                Layer::Repository,
                Layer::Service,
                Layer::Ui,
            ],
        }
    }
}

pub fn validate_dependency_rules(architecture: &Architecture) -> Vec<String> {
    let mut issues = Vec::new();
    let units = architecture.design_units_by_id();

    for dependency in &architecture.dependencies {
        let Some(from_unit) = units.get(&dependency.from.0) else {
            continue;
        };
        let Some(to_unit) = units.get(&dependency.to.0) else {
            continue;
        };

        if from_unit.layer.order() > to_unit.layer.order() {
            issues.push(format!(
                "forbidden dependency: {} -> {}",
                from_unit.layer.as_str(),
                to_unit.layer.as_str()
            ));
        }
    }

    issues
}
