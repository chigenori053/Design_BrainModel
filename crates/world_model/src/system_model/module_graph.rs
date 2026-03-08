use design_domain::Architecture;

pub fn module_coupling_score(architecture: &Architecture) -> f64 {
    let classes = architecture.classes.len().max(1) as f64;
    let dependencies = architecture.dependencies.len() as f64;
    (1.0 - (dependencies / (classes * 4.0)).clamp(0.0, 1.0)).clamp(0.0, 1.0)
}
