use design_domain::DesignUnitId;

use crate::Concept;

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticUnit {
    pub id: u64,
    pub concept: Concept,
    pub mapped_design_unit: Option<DesignUnitId>,
}
