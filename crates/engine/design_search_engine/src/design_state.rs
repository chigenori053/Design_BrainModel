use memory_space_complex::ComplexField;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct DesignStateId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct DesignUnitId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DesignUnitType {
    ClassUnit,
    StructureUnit,
    DesignUnit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignUnit {
    pub id: DesignUnitId,
    pub unit_type: DesignUnitType,
    pub dependencies: Vec<DesignUnitId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EvaluationScore {
    pub structural: f64,
    pub dependency: f64,
    pub concept_alignment: f64,
}

impl EvaluationScore {
    pub fn total(&self) -> f64 {
        0.4 * self.structural + 0.3 * self.dependency + 0.3 * self.concept_alignment
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesignState {
    pub id: DesignStateId,
    pub design_units: Vec<DesignUnit>,
    pub evaluation: Option<EvaluationScore>,
    pub state_vector: ComplexField,
}
