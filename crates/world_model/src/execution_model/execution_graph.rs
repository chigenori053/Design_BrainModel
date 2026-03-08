use design_domain::Architecture;

pub fn execution_complexity(architecture: &Architecture) -> f64 {
    let units = architecture.design_unit_count() as f64;
    let dependencies = architecture.dependencies.len() as f64;
    ((units + dependencies) / 16.0).clamp(0.0, 1.0)
}
