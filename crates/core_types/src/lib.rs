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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayerKind {
    Orchestration,
    Design,
    Semantic,
    Execution,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassUnit {
    pub id: String,
    pub name: String,
    pub fields: Vec<String>,
    pub methods: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureUnit {
    pub id: String,
    pub name: String,
    pub classes: Vec<ClassUnit>,
    pub dependencies: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignUnit {
    pub id: String,
    pub name: String,
    pub structures: Vec<StructureUnit>,
}

pub type DesignId = String;
pub type ClassId = String;
pub type StructureId = String;
pub type UnitId = String;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitRole {
    Interface,
    Implementation,
    Domain,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependencyKind {
    Uses,
    Owns,
    Extends,
    Constrains,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitNode {
    pub id: UnitId,
    pub role: UnitRole,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureNode {
    pub id: StructureId,
    pub units: Vec<UnitNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassNode {
    pub id: ClassId,
    pub structures: Vec<StructureNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignHierarchy {
    pub classes: Vec<ClassNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyEdge {
    pub from: UnitId,
    pub to: UnitId,
    pub kind: DependencyKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencyGraph {
    pub edges: Vec<DependencyEdge>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResourceSpec {
    pub resource: String,
    pub limit: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeSpec {
    pub target: String,
    pub required_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Constraint {
    Invariant(String),
    ResourceLimit(ResourceSpec),
    TypeRequirement(TypeSpec),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectiveKind {
    Performance,
    Safety,
    Readability,
    MemoryEfficiency,
    Determinism,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesignIntent {
    pub objective: ObjectiveKind,
    pub description: String,
    pub priority: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeFrontier {
    pub mutable_units: Vec<UnitId>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesignIR {
    pub id: DesignId,
    pub hierarchy: DesignHierarchy,
    pub dependencies: DependencyGraph,
    pub constraints: Vec<Constraint>,
    pub intent: DesignIntent,
    pub frontier: ChangeFrontier,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticIR {
    pub concepts: Vec<String>,
    pub dependency_graph: Vec<(usize, usize)>,
    pub constraints: Vec<String>,
    pub objective: Option<ObjectiveKind>,
    pub mutable_concepts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NumericIR {
    pub features: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NumericResult {
    pub values: Vec<f64>,
}

pub trait DesignCompiler {
    fn to_ir(&self, design: &DesignUnit) -> DesignIR;
}

pub trait SemanticLowering {
    fn to_semantic_ir(&self, design_ir: &DesignIR) -> SemanticIR;
}

pub trait NumericLowering {
    fn to_numeric_ir(&self, semantic_ir: &SemanticIR) -> NumericIR;
}

pub trait NumericEvaluator {
    fn evaluate(&self, input: &NumericIR) -> NumericResult;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DesignIRDiff {
    pub added_units: Vec<UnitId>,
    pub removed_units: Vec<UnitId>,
    pub changed_intent: bool,
}

pub fn diff_design_ir(previous: &DesignIR, current: &DesignIR) -> DesignIRDiff {
    let mut prev_units = previous.frontier.mutable_units.clone();
    prev_units.sort();
    prev_units.dedup();

    let mut cur_units = current.frontier.mutable_units.clone();
    cur_units.sort();
    cur_units.dedup();

    let added_units = cur_units
        .iter()
        .filter(|u| !prev_units.contains(u))
        .cloned()
        .collect::<Vec<_>>();
    let removed_units = prev_units
        .iter()
        .filter(|u| !cur_units.contains(u))
        .cloned()
        .collect::<Vec<_>>();

    DesignIRDiff {
        added_units,
        removed_units,
        changed_intent: previous.intent != current.intent,
    }
}

pub fn lower_design_to_numeric<C, S, N>(
    compiler: &C,
    semantic_lowering: &S,
    numeric_lowering: &N,
    design: &DesignUnit,
) -> NumericIR
where
    C: DesignCompiler,
    S: SemanticLowering,
    N: NumericLowering,
{
    let design_ir = compiler.to_ir(design);
    let semantic_ir = semantic_lowering.to_semantic_ir(&design_ir);
    numeric_lowering.to_numeric_ir(&semantic_ir)
}

#[cfg(test)]
mod tests {
    use super::{
        ChangeFrontier, ClassNode, Constraint, DependencyGraph, DesignCompiler, DesignHierarchy,
        DesignIR, DesignIntent, DesignUnit, NumericIR, NumericLowering, ObjectiveKind, SemanticIR,
        SemanticLowering, StructureNode, StructureUnit, UnitNode, UnitRole, diff_design_ir,
        lower_design_to_numeric,
    };

    #[derive(Default)]
    struct DummyDesignCompiler;

    impl DesignCompiler for DummyDesignCompiler {
        fn to_ir(&self, design: &DesignUnit) -> DesignIR {
            DesignIR {
                id: design.id.clone(),
                hierarchy: DesignHierarchy {
                    classes: vec![ClassNode {
                        id: format!("class:{}", design.name),
                        structures: vec![StructureNode {
                            id: format!("structure:{}", design.name),
                            units: vec![UnitNode {
                                id: format!("unit:{}", design.name),
                                role: UnitRole::Implementation,
                            }],
                        }],
                    }],
                },
                dependencies: DependencyGraph { edges: Vec::new() },
                constraints: vec![Constraint::Invariant("dummy".to_string())],
                intent: DesignIntent {
                    objective: ObjectiveKind::Readability,
                    description: design.name.clone(),
                    priority: 1.0,
                },
                frontier: ChangeFrontier {
                    mutable_units: vec![format!("unit:{}", design.name)],
                },
            }
        }
    }

    #[derive(Default)]
    struct DummySemanticLowering;

    impl SemanticLowering for DummySemanticLowering {
        fn to_semantic_ir(&self, design_ir: &DesignIR) -> SemanticIR {
            SemanticIR {
                concepts: design_ir.frontier.mutable_units.clone(),
                dependency_graph: Vec::new(),
                constraints: vec!["dummy".to_string()],
                objective: Some(design_ir.intent.objective),
                mutable_concepts: design_ir.frontier.mutable_units.clone(),
            }
        }
    }

    #[derive(Default)]
    struct DummyNumericLowering;

    impl NumericLowering for DummyNumericLowering {
        fn to_numeric_ir(&self, semantic_ir: &SemanticIR) -> NumericIR {
            NumericIR {
                features: vec![semantic_ir.concepts.len() as f64],
            }
        }
    }

    #[test]
    fn design_to_numeric_requires_two_stage_lowering() {
        let design = DesignUnit {
            id: "d1".to_string(),
            name: "ServiceDesign".to_string(),
            structures: vec![StructureUnit {
                id: "s1".to_string(),
                name: "Core".to_string(),
                classes: Vec::new(),
                dependencies: Vec::new(),
            }],
        };

        let numeric = lower_design_to_numeric(
            &DummyDesignCompiler,
            &DummySemanticLowering,
            &DummyNumericLowering,
            &design,
        );

        assert_eq!(numeric.features, vec![1.0]);
    }

    #[test]
    fn design_ir_diff_tracks_frontier_and_intent() {
        let mut a = DummyDesignCompiler.to_ir(&DesignUnit {
            id: "d1".to_string(),
            name: "A".to_string(),
            structures: vec![StructureUnit {
                id: "s1".to_string(),
                name: "core".to_string(),
                classes: Vec::new(),
                dependencies: Vec::new(),
            }],
        });
        let mut b = a.clone();
        b.frontier.mutable_units.push("unit:new".to_string());
        b.intent.objective = ObjectiveKind::Performance;
        a.frontier.mutable_units.push("unit:old".to_string());

        let diff = diff_design_ir(&a, &b);
        assert_eq!(diff.added_units, vec!["unit:new".to_string()]);
        assert_eq!(diff.removed_units, vec!["unit:old".to_string()]);
        assert!(diff.changed_intent);
    }
}
