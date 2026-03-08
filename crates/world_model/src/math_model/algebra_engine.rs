use design_domain::Architecture;

pub fn algebraic_stability(architecture: &Architecture) -> f64 {
    let units = architecture.design_unit_count() as f64;
    let structures = architecture.structure_count().max(1) as f64;
    (1.0 - ((units - structures).abs() / (units + structures + 1.0))).clamp(0.0, 1.0)
}
