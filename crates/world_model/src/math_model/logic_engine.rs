use design_domain::Architecture;

use crate::system_model::dependency_cycle_count;

pub fn logic_verification_score(architecture: &Architecture) -> f64 {
    let cycles = dependency_cycle_count(architecture) as f64;
    (1.0 - (cycles / 4.0)).clamp(0.0, 1.0)
}
