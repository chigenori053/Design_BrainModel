use evaluator::ObjectiveVector;

#[derive(Clone, Debug, PartialEq)]
pub struct PreferenceProfile {
    pub struct_weight: f64,
    pub field_weight: f64,
    pub risk_weight: f64,
    pub cost_weight: f64,
}

impl PreferenceProfile {
    pub fn normalized(self) -> Self {
        let sum = (self.struct_weight + self.field_weight + self.risk_weight + self.cost_weight).max(1e-12);
        Self {
            struct_weight: self.struct_weight / sum,
            field_weight: self.field_weight / sum,
            risk_weight: self.risk_weight / sum,
            cost_weight: self.cost_weight / sum,
        }
    }

    pub fn score(&self, obj: &ObjectiveVector) -> f64 {
        let n = self.clone().normalized();
        (n.struct_weight * obj.f_struct
            + n.field_weight * obj.f_field
            + n.risk_weight * obj.f_risk
            + n.cost_weight * obj.f_cost)
            .clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileSnapshot {
    pub generation: usize,
    pub blended: PreferenceProfile,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileManager {
    current_generation: usize,
    current: PreferenceProfile,
    snapshots: Vec<ProfileSnapshot>,
}

impl ProfileManager {
    pub fn new(initial: PreferenceProfile) -> Self {
        let normalized = initial.normalized();
        Self {
            current_generation: 0,
            current: normalized.clone(),
            snapshots: vec![ProfileSnapshot {
                generation: 0,
                blended: normalized,
            }],
        }
    }

    pub fn current(&self) -> &PreferenceProfile {
        &self.current
    }

    pub fn snapshots(&self) -> &[ProfileSnapshot] {
        &self.snapshots
    }

    pub fn update_generation(
        &mut self,
        user: &PreferenceProfile,
        auto: &PreferenceProfile,
        lambda: f64,
        max_delta: f64,
    ) {
        let target = blend_profiles(user, auto, lambda);
        self.current_generation += 1;
        self.current = step_towards(&self.current, &target, max_delta.clamp(0.0, 1.0));
        self.snapshots.push(ProfileSnapshot {
            generation: self.current_generation,
            blended: self.current.clone(),
        });
    }
}

pub fn blend_profiles(user: &PreferenceProfile, auto: &PreferenceProfile, lambda: f64) -> PreferenceProfile {
    let l = lambda.clamp(0.0, 1.0);
    PreferenceProfile {
        struct_weight: l * user.struct_weight + (1.0 - l) * auto.struct_weight,
        field_weight: l * user.field_weight + (1.0 - l) * auto.field_weight,
        risk_weight: l * user.risk_weight + (1.0 - l) * auto.risk_weight,
        cost_weight: l * user.cost_weight + (1.0 - l) * auto.cost_weight,
    }
    .normalized()
}

fn step_towards(current: &PreferenceProfile, target: &PreferenceProfile, max_delta: f64) -> PreferenceProfile {
    PreferenceProfile {
        struct_weight: move_axis(current.struct_weight, target.struct_weight, max_delta),
        field_weight: move_axis(current.field_weight, target.field_weight, max_delta),
        risk_weight: move_axis(current.risk_weight, target.risk_weight, max_delta),
        cost_weight: move_axis(current.cost_weight, target.cost_weight, max_delta),
    }
    .normalized()
}

fn move_axis(current: f64, target: f64, max_delta: f64) -> f64 {
    let delta = (target - current).clamp(-max_delta, max_delta);
    current + delta
}

#[cfg(test)]
mod tests {
    use evaluator::ObjectiveVector;

    use crate::{blend_profiles, PreferenceProfile, ProfileManager};

    #[test]
    fn blend_is_clamped_and_normalized() {
        let user = PreferenceProfile {
            struct_weight: 1.0,
            field_weight: 0.0,
            risk_weight: 0.0,
            cost_weight: 0.0,
        };
        let auto = PreferenceProfile {
            struct_weight: 0.0,
            field_weight: 1.0,
            risk_weight: 0.0,
            cost_weight: 0.0,
        };

        let p = blend_profiles(&user, &auto, 2.0);
        assert_eq!(p.struct_weight, 1.0);

        let s = p.score(&ObjectiveVector {
            f_struct: 1.0,
            f_field: 0.0,
            f_risk: 0.0,
            f_cost: 0.0,
        });
        assert_eq!(s, 1.0);
    }

    #[test]
    fn manager_updates_by_generation_with_delta_cap() {
        let initial = PreferenceProfile {
            struct_weight: 1.0,
            field_weight: 0.0,
            risk_weight: 0.0,
            cost_weight: 0.0,
        };
        let user = PreferenceProfile {
            struct_weight: 0.0,
            field_weight: 1.0,
            risk_weight: 0.0,
            cost_weight: 0.0,
        };
        let auto = PreferenceProfile {
            struct_weight: 0.0,
            field_weight: 1.0,
            risk_weight: 0.0,
            cost_weight: 0.0,
        };
        let mut manager = ProfileManager::new(initial);
        manager.update_generation(&user, &auto, 0.5, 0.1);

        assert_eq!(manager.snapshots().len(), 2);
        assert_eq!(manager.snapshots()[1].generation, 1);
        assert!(manager.current().struct_weight > 0.0);
        assert!(manager.current().field_weight > 0.0);
    }
}
