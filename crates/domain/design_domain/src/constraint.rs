use crate::Architecture;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constraint {
    pub name: String,
    pub max_design_units: Option<usize>,
    pub max_dependencies: Option<usize>,
}

impl Constraint {
    pub fn satisfied_by(&self, architecture: &Architecture) -> bool {
        let unit_ok = self
            .max_design_units
            .map(|limit| architecture.design_unit_count() <= limit)
            .unwrap_or(true);
        let dependency_ok = self
            .max_dependencies
            .map(|limit| architecture.dependencies.len() <= limit)
            .unwrap_or(true);

        unit_ok && dependency_ok
    }
}
