use design_domain::Architecture;

pub fn estimate_dependency_cost(architecture: &Architecture) -> f64 {
    (architecture.dependencies.len() as f64 / 8.0).clamp(0.0, 1.0)
}
