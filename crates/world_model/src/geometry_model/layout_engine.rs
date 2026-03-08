use design_domain::Architecture;

pub fn layout_balance_score(architecture: &Architecture) -> f64 {
    if architecture.classes.is_empty() {
        return 1.0;
    }

    let total_structures = architecture.structure_count() as f64;
    let mean = total_structures / architecture.classes.len() as f64;
    let imbalance = architecture
        .classes
        .iter()
        .map(|class_unit| (class_unit.structures.len() as f64 - mean).abs())
        .sum::<f64>();

    (1.0 - imbalance / (total_structures + 1.0)).clamp(0.0, 1.0)
}
