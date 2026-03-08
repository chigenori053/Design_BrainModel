use design_domain::Architecture;

pub fn estimate_memory_usage(architecture: &Architecture) -> f64 {
    let units = architecture.design_unit_count() as f64;
    let structures = architecture.structure_count() as f64;
    ((units * 0.6 + structures * 0.4) / 12.0).clamp(0.0, 1.0)
}
