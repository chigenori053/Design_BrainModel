use core_types::{ObjectiveVector, P_INFER_ALPHA, P_INFER_BETA, P_INFER_GAMMA};
use profile::PreferenceProfile;

pub fn p_inferred(
    p_shm: &PreferenceProfile,
    p_pareto: &PreferenceProfile,
    p_chm: &PreferenceProfile,
    prev: &PreferenceProfile,
) -> PreferenceProfile {
    let raw = PreferenceProfile {
        struct_weight: P_INFER_ALPHA * p_shm.struct_weight
            + P_INFER_BETA * p_pareto.struct_weight
            + P_INFER_GAMMA * p_chm.struct_weight,
        field_weight: P_INFER_ALPHA * p_shm.field_weight
            + P_INFER_BETA * p_pareto.field_weight
            + P_INFER_GAMMA * p_chm.field_weight,
        risk_weight: P_INFER_ALPHA * p_shm.risk_weight
            + P_INFER_BETA * p_pareto.risk_weight
            + P_INFER_GAMMA * p_chm.risk_weight,
        cost_weight: P_INFER_ALPHA * p_shm.cost_weight
            + P_INFER_BETA * p_pareto.cost_weight
            + P_INFER_GAMMA * p_chm.cost_weight,
    }
    .normalized();

    PreferenceProfile {
        struct_weight: (1.0 - 0.2) * prev.struct_weight + 0.2 * raw.struct_weight,
        field_weight: (1.0 - 0.2) * prev.field_weight + 0.2 * raw.field_weight,
        risk_weight: (1.0 - 0.2) * prev.risk_weight + 0.2 * raw.risk_weight,
        cost_weight: (1.0 - 0.2) * prev.cost_weight + 0.2 * raw.cost_weight,
    }
    .normalized()
}

pub fn need_from_objective(obj: &ObjectiveVector) -> PreferenceProfile {
    PreferenceProfile {
        struct_weight: 1.0 - obj.f_struct,
        field_weight: 1.0 - obj.f_field,
        risk_weight: 1.0 - obj.f_risk,
        cost_weight: 1.0 - obj.f_shape,
    }
    .normalized()
}

pub fn stability_index(
    high_reliability: f64,
    safety_critical: f64,
    experimental: f64,
    rapid_prototype: f64,
) -> f64 {
    core_types::stability_index(
        high_reliability,
        safety_critical,
        experimental,
        rapid_prototype,
    )
}

pub fn chm_density(n_edge_obs: usize, category_count: usize) -> f64 {
    if category_count <= 1 {
        return 0.0;
    }
    let denom = (category_count * (category_count - 1)) as f64;
    (n_edge_obs as f64 / denom).clamp(0.0, 1.0)
}

pub fn profile_modulation(stability_index: f64) -> f64 {
    let s = stability_index.clamp(-1.0, 1.0);
    let sigma = 1.0 / (1.0 + (-1.5 * s).exp());
    0.85 + (1.20 - 0.85) * sigma
}
