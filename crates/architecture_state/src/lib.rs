use architecture_reasoner::ArchitectureGraph;
use code_ir::CodeIr;
use design_domain::Constraint;
use geometry_engine::GeometryReport;
use knowledge_engine::KnowledgeIntegration;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArchitectureEvaluation {
    pub geometry: GeometryReport,
    pub knowledge_alignment: f64,
    pub overall: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArchitectureState {
    pub problem: String,
    pub knowledge: Option<KnowledgeIntegration>,
    pub constraints: Vec<Constraint>,
    pub architecture_graph: ArchitectureGraph,
    pub code_ir: CodeIr,
    pub evaluation: Option<ArchitectureEvaluation>,
}

impl ArchitectureState {
    pub fn new(problem: impl Into<String>) -> Self {
        Self {
            problem: problem.into(),
            ..Self::default()
        }
    }

    pub fn with_knowledge(mut self, knowledge: KnowledgeIntegration) -> Self {
        self.knowledge = Some(knowledge);
        self
    }

    pub fn with_constraints(mut self, constraints: Vec<Constraint>) -> Self {
        self.constraints = constraints;
        self
    }

    pub fn stabilize_knowledge_constraints(&self) -> Vec<String> {
        self.knowledge
            .as_ref()
            .map(|knowledge| {
                knowledge
                    .knowledge_graph
                    .relations
                    .iter()
                    .map(|relation| format!("{:?}", relation.relation_type).to_ascii_lowercase())
                    .collect()
            })
            .unwrap_or_default()
    }
}
