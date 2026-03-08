use world_model_core::WorldState;

pub fn constraint_solver_score(state: &WorldState) -> f64 {
    if state.constraints.is_empty() {
        return 1.0;
    }

    let satisfied = state
        .constraints
        .iter()
        .filter(|constraint| constraint.satisfied_by(&state.architecture))
        .count();

    (satisfied as f64 / state.constraints.len() as f64).clamp(0.0, 1.0)
}
