#[derive(Clone, Debug, PartialEq)]
pub struct ObjectiveVector {
    pub f_struct: f64,
    pub f_field: f64,
    pub f_risk: f64,
    pub f_shape: f64,
}

impl ObjectiveVector {
    pub fn clamped(self) -> Self {
        Self {
            f_struct: self.f_struct.clamp(0.0, 1.0),
            f_field: self.f_field.clamp(0.0, 1.0),
            f_risk: self.f_risk.clamp(0.0, 1.0),
            f_shape: self.f_shape.clamp(0.0, 1.0),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProfileVector {
    pub struct_weight: f64,
    pub field_weight: f64,
    pub risk_weight: f64,
    pub cost_weight: f64,
}

impl ProfileVector {
    pub fn normalized(self) -> Self {
        let sum = (self.struct_weight + self.field_weight + self.risk_weight + self.cost_weight)
            .max(1e-12);
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
            + n.cost_weight * obj.f_shape)
            .clamp(0.0, 1.0)
    }
}

pub const P_INFER_ALPHA: f64 = 0.4;
pub const P_INFER_BETA: f64 = 0.3;
pub const P_INFER_GAMMA: f64 = 0.3;

pub fn stability_index(
    high_reliability: f64,
    safety_critical: f64,
    experimental: f64,
    rapid_prototype: f64,
) -> f64 {
    (high_reliability + safety_critical - experimental - rapid_prototype).clamp(-1.0, 1.0)
}
