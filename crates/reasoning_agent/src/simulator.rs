use memory_space_complex::{ComplexField, bind, normalize};

use crate::types::Hypothesis;

const STATE_BOUND: f64 = 1.0;

pub fn simulate(state: &ComplexField, action: &Hypothesis) -> ComplexField {
    let mut next = bind(state, &action.action_vector);
    enforce_state_bound(&mut next);
    next
}

fn enforce_state_bound(state: &mut ComplexField) {
    let norm = state
        .data
        .iter()
        .map(|z| z.norm_sqr() as f64)
        .sum::<f64>()
        .sqrt();
    if norm > STATE_BOUND {
        normalize(state);
    }
}
