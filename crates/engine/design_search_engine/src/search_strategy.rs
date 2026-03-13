use causal_domain::{CausalRelation, CausalRelationKind};
use memory_space_complex::{ComplexField, normalize};
use memory_space_core::Complex64;

use crate::design_state::{DesignState, DesignStateId, DesignUnit, DesignUnitId, DesignUnitType};

pub trait SearchStrategy {
    fn expand(&self, state: &DesignState) -> Vec<DesignState>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BeamSearchStrategy;

impl SearchStrategy for BeamSearchStrategy {
    fn expand(&self, state: &DesignState) -> Vec<DesignState> {
        let mut out = Vec::with_capacity(4);

        let mut add = state.clone();
        let new_unit_id = DesignUnitId(state.design_units.len() as u64 + 1);
        add.design_units.push(DesignUnit {
            id: new_unit_id,
            unit_type: match state.design_units.len() % 3 {
                0 => DesignUnitType::ClassUnit,
                1 => DesignUnitType::StructureUnit,
                _ => DesignUnitType::DesignUnit,
            },
            dependencies: if state.design_units.is_empty() {
                Vec::new()
            } else {
                vec![state.design_units[0].id]
            },
            causal_relations: if state.design_units.is_empty() {
                Vec::new()
            } else {
                vec![CausalRelation {
                    target: state.design_units[0].id.0,
                    kind: CausalRelationKind::Requires,
                }]
            },
        });
        add.id = DesignStateId(state.id.0.wrapping_mul(31).wrapping_add(1));
        add.state_vector = next_vector(&state.state_vector, 0.1);
        out.push(add);

        let mut remove = state.clone();
        if !remove.design_units.is_empty() {
            remove.design_units.pop();
        }
        remove.id = DesignStateId(state.id.0.wrapping_mul(31).wrapping_add(2));
        remove.state_vector = next_vector(&state.state_vector, -0.1);
        out.push(remove);

        let mut modify = state.clone();
        if let Some(first) = modify.design_units.first_mut() {
            first.dependencies.reverse();
        }
        modify.id = DesignStateId(state.id.0.wrapping_mul(31).wrapping_add(3));
        modify.state_vector = next_vector(&state.state_vector, 0.2);
        out.push(modify);

        let mut refactor = state.clone();
        refactor.design_units.sort_by_key(|unit| unit.id.0);
        refactor.id = DesignStateId(state.id.0.wrapping_mul(31).wrapping_add(4));
        refactor.state_vector = next_vector(&state.state_vector, 0.05);
        out.push(refactor);

        out
    }
}

fn next_vector(prev: &ComplexField, delta: f32) -> ComplexField {
    let mut data = if prev.data.is_empty() {
        vec![Complex64::new(1.0, 0.0); 8]
    } else {
        prev.data.clone()
    };

    for value in &mut data {
        value.re += delta;
    }
    let mut field = ComplexField::new(data);
    normalize(&mut field);
    field
}
