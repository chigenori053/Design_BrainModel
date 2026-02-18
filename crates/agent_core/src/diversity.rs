use field_engine::{FieldVector, TargetField};

pub const DIVERSITY_EPSILON: f64 = 0.1;
pub const DIVERSITY_EPSILON_MAX: f64 = 0.15;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiversityAdjustment {
    pub pressure: f64,
    pub epsilon_effect: f64,
    pub target_local_weight: f64,
    pub target_global_weight: f64,
    pub local_global_distance: f64,
}

pub fn pressure_from_distance(distance: f64, pressure_lambda: f64) -> f64 {
    if !distance.is_finite() || !pressure_lambda.is_finite() {
        return 0.0;
    }
    let d = distance.max(0.0);
    let lambda = pressure_lambda.max(0.0);
    (-lambda * d).exp().clamp(0.0, 1.0)
}

pub fn epsilon_effect(pressure: f64) -> f64 {
    (DIVERSITY_EPSILON * pressure.clamp(0.0, 1.0)).clamp(0.0, DIVERSITY_EPSILON_MAX)
}

pub fn apply_diversity_pressure(
    base: &TargetField,
    global: &FieldVector,
    local: &FieldVector,
    lambda: f64,
    pressure_lambda: f64,
) -> (TargetField, DiversityAdjustment) {
    let global_weight = lambda.clamp(0.0, 1.0);
    let local_weight = 1.0 - global_weight;
    let d_local = l2_distance(&base.data, local);
    let d_global = l2_distance(&base.data, global);
    let integrated_distance = local_weight * d_local + global_weight * d_global;
    let pressure = pressure_from_distance(integrated_distance, pressure_lambda);
    let epsilon_effect = epsilon_effect(pressure);
    let e = epsilon_effect as f32;

    let adjusted = TargetField {
        data: base.data.scale(1.0 - e).add(&local.scale(e)),
    };
    let target_global_weight = global_weight * (1.0 - epsilon_effect);
    let target_local_weight = local_weight * (1.0 - epsilon_effect) + epsilon_effect;
    let local_global_distance = l2_distance(global, local);

    (
        adjusted,
        DiversityAdjustment {
            pressure,
            epsilon_effect,
            target_local_weight,
            target_global_weight,
            local_global_distance,
        },
    )
}

fn l2_distance(a: &FieldVector, b: &FieldVector) -> f64 {
    let len = a.dimensions().min(b.dimensions());
    let mut sum = 0.0f64;
    for i in 0..len {
        let diff = a.data[i] - b.data[i];
        sum += diff.norm_sqr() as f64;
    }
    sum.sqrt()
}

#[cfg(test)]
mod tests {
    use field_engine::{FieldEngine, NodeCategory, TargetField};

    use super::{
        DIVERSITY_EPSILON, DIVERSITY_EPSILON_MAX, apply_diversity_pressure, epsilon_effect,
        pressure_from_distance,
    };

    #[test]
    fn pressure_is_monotonic() {
        let high = pressure_from_distance(0.0, 1.0);
        let low = pressure_from_distance(0.2, 1.0);
        assert!(high >= low);
        assert!((0.0..=1.0).contains(&high));
        assert!((0.0..=1.0).contains(&low));
    }

    #[test]
    fn pressure_is_convex_like_exponential_decay() {
        let p0 = pressure_from_distance(0.0, 1.0);
        let p1 = pressure_from_distance(0.5, 1.0);
        let p2 = pressure_from_distance(1.0, 1.0);
        assert!(p0 >= p1 && p1 >= p2);
        // midpoint of exp(-x) lies below linear interpolation in [0,1].
        assert!(p1 <= 0.5 * (p0 + p2) + 1e-12);
    }

    #[test]
    fn epsilon_effect_is_bounded() {
        let v = epsilon_effect(1.0);
        assert!(v <= DIVERSITY_EPSILON_MAX + 1e-12);
        assert!((DIVERSITY_EPSILON - v).abs() < 1e-12 || v <= DIVERSITY_EPSILON);
    }

    #[test]
    fn adjustment_reports_weights_and_distance() {
        let field = FieldEngine::new(8);
        let g = field.projector().basis_for(NodeCategory::Interface);
        let l = field.projector().basis_for(NodeCategory::Storage);
        let base = TargetField {
            data: g.scale(0.5).add(&l.scale(0.5)),
        };
        let (_, adj) = apply_diversity_pressure(&base, &g, &l, 0.5, 0.0);
        assert!((adj.target_global_weight + adj.target_local_weight - 1.0).abs() < 1e-12);
        assert!(adj.local_global_distance > 0.0);
    }
}
