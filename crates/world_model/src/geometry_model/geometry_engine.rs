use design_domain::Architecture;

pub fn graph_layout_score(architecture: &Architecture) -> f64 {
    let nodes = architecture.classes.len() as f64 + architecture.structure_count() as f64;
    let edges = architecture.dependencies.len() as f64;
    if nodes == 0.0 {
        return 1.0;
    }
    (1.0 - (edges / (nodes * 3.0)).clamp(0.0, 1.0)).clamp(0.0, 1.0)
}
