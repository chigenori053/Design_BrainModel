use core_types::ObjectiveVector;

pub fn objective_l2_distance(a: &ObjectiveVector, b: &ObjectiveVector) -> f64 {
    let ds = a.f_struct - b.f_struct;
    let df = a.f_field - b.f_field;
    let dr = a.f_risk - b.f_risk;
    let dc = a.f_shape - b.f_shape;
    (ds * ds + df * df + dr * dr + dc * dc).sqrt()
}
