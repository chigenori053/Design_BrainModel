use architecture_domain::ArchitectureState;
use evaluation_engine::EvaluationResult;
use semantic_domain::MeaningGraph;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct ProblemId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct ArchitectureId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct EvaluationId(pub u64);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProblemNode {
    pub problem_id: ProblemId,
    pub semantic_graph: MeaningGraph,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArchitectureNode {
    pub architecture_id: ArchitectureId,
    pub architecture_hash: u64,
    pub architecture: ArchitectureState,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationNode {
    pub evaluation_id: EvaluationId,
    pub result: EvaluationResult,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExperienceEdge {
    pub problem_id: ProblemId,
    pub architecture_id: ArchitectureId,
    pub evaluation_id: EvaluationId,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DesignExperienceGraph {
    pub problems: Vec<ProblemNode>,
    pub architectures: Vec<ArchitectureNode>,
    pub evaluations: Vec<EvaluationNode>,
    pub edges: Vec<ExperienceEdge>,
}

impl DesignExperienceGraph {
    pub fn record_experience(
        &mut self,
        semantic_graph: MeaningGraph,
        architecture_hash: u64,
        architecture: ArchitectureState,
        result: EvaluationResult,
    ) {
        let problem_id = ProblemId(self.problems.len() as u64 + 1);
        let architecture_id = ArchitectureId(self.architectures.len() as u64 + 1);
        let evaluation_id = EvaluationId(self.evaluations.len() as u64 + 1);
        self.problems.push(ProblemNode {
            problem_id,
            semantic_graph,
        });
        self.architectures.push(ArchitectureNode {
            architecture_id,
            architecture_hash,
            architecture,
        });
        self.evaluations.push(EvaluationNode {
            evaluation_id,
            result,
        });
        self.edges.push(ExperienceEdge {
            problem_id,
            architecture_id,
            evaluation_id,
        });
    }
}
