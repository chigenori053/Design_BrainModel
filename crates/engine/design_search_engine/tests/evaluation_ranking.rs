use concept_engine::ConceptId;
use design_search_engine::{
    DesignState, DesignStateId, DesignUnit, DesignUnitId, DesignUnitType, Evaluator,
};
use memory_space_complex::ComplexField;

#[test]
fn evaluation_ranking() {
    let evaluator = Evaluator;
    let concepts = vec![ConceptId::from_name("A"), ConceptId::from_name("B")];

    let weak = DesignState {
        id: DesignStateId(1),
        design_units: vec![DesignUnit {
            id: DesignUnitId(1),
            unit_type: DesignUnitType::ClassUnit,
            dependencies: Vec::new(),
        }],
        evaluation: None,
        state_vector: ComplexField::new(Vec::new()),
    };

    let strong = DesignState {
        id: DesignStateId(2),
        design_units: vec![
            DesignUnit {
                id: DesignUnitId(1),
                unit_type: DesignUnitType::ClassUnit,
                dependencies: Vec::new(),
            },
            DesignUnit {
                id: DesignUnitId(2),
                unit_type: DesignUnitType::StructureUnit,
                dependencies: vec![DesignUnitId(1)],
            },
        ],
        evaluation: None,
        state_vector: ComplexField::new(Vec::new()),
    };

    let s_weak = evaluator.evaluate(&weak, &concepts).total();
    let s_strong = evaluator.evaluate(&strong, &concepts).total();
    assert!(s_strong >= s_weak);
}
