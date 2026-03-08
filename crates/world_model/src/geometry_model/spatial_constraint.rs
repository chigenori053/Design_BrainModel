use world_model_core::WorldState;

pub fn spatial_constraint_score(state: &WorldState) -> f64 {
    let layout_density = state.architecture.dependencies.len() as f64
        / (state.architecture.structure_count().max(1) as f64 + 1.0);
    (1.0 - (layout_density / 3.0)).clamp(0.0, 1.0)
}
