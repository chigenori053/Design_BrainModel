use semantic_dhm::{ConceptUnit, DesignProjection, SemanticUnitL1};

#[derive(Clone, Default)]
pub struct ProjectionEngine;

impl ProjectionEngine {
    pub fn project_phase_a(
        &self,
        l2_units: &[ConceptUnit],
        l1_units: &[SemanticUnitL1],
    ) -> DesignProjection {
        semantic_dhm::project_phase_a(l2_units, l1_units)
    }
}
