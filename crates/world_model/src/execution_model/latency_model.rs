use design_domain::Architecture;

pub fn estimate_latency_score(architecture: &Architecture) -> f64 {
    let structures = architecture.structure_count().max(1) as f64;
    let dependencies = architecture.dependencies.len() as f64;
    (1.0 - (dependencies / (structures * 4.0)).clamp(0.0, 1.0)).clamp(0.0, 1.0)
}
