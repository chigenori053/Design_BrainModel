use architecture_domain::ArchitectureState;
use evaluation_engine::EvaluationResult;
use knowledge_engine::{KnowledgeGraph, ValidationScore};
use knowledge_lifecycle::{KnowledgeLifecycleState, LifecycleMetrics};
use semantic_domain::MeaningGraph;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct ProblemId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct ArchitectureId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct EvaluationId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct KnowledgeId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct InferredKnowledgeId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct StabilizedKnowledgeId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct LifecycleStateId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct LifecycleMetricsId(pub u64);

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
pub struct KnowledgeNode {
    pub knowledge_id: KnowledgeId,
    pub graph: KnowledgeGraph,
    pub validation: ValidationScore,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InferredKnowledgeNode {
    pub inferred_knowledge_id: InferredKnowledgeId,
    pub graph: KnowledgeGraph,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StabilizedKnowledgeNode {
    pub stabilized_knowledge_id: StabilizedKnowledgeId,
    pub graph: KnowledgeGraph,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LifecycleStateNode {
    pub lifecycle_state_id: LifecycleStateId,
    pub state: KnowledgeLifecycleState,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LifecycleMetricsNode {
    pub lifecycle_metrics_id: LifecycleMetricsId,
    pub metrics: LifecycleMetrics,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ExperienceEdge {
    pub problem_id: ProblemId,
    pub knowledge_id: Option<KnowledgeId>,
    pub inferred_knowledge_id: Option<InferredKnowledgeId>,
    pub stabilized_knowledge_id: Option<StabilizedKnowledgeId>,
    pub lifecycle_state_id: Option<LifecycleStateId>,
    pub lifecycle_metrics_id: Option<LifecycleMetricsId>,
    pub architecture_id: ArchitectureId,
    pub evaluation_id: EvaluationId,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DesignExperienceGraph {
    pub problems: Vec<ProblemNode>,
    pub knowledges: Vec<KnowledgeNode>,
    pub inferred_knowledges: Vec<InferredKnowledgeNode>,
    pub stabilized_knowledges: Vec<StabilizedKnowledgeNode>,
    pub lifecycle_states: Vec<LifecycleStateNode>,
    pub lifecycle_metrics: Vec<LifecycleMetricsNode>,
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
        self.record_experience_with_knowledge(
            semantic_graph,
            None,
            None,
            architecture_hash,
            architecture,
            result,
        );
    }

    pub fn record_experience_with_knowledge(
        &mut self,
        semantic_graph: MeaningGraph,
        knowledge_graph: Option<KnowledgeGraph>,
        validation: Option<ValidationScore>,
        architecture_hash: u64,
        architecture: ArchitectureState,
        result: EvaluationResult,
    ) {
        let inferred = knowledge_graph.clone();
        let stabilized = knowledge_graph.clone();
        let lifecycle_state = KnowledgeLifecycleState {
            ..KnowledgeLifecycleState::default()
        };
        self.record_experience_with_lifecycle(
            semantic_graph,
            knowledge_graph,
            validation,
            inferred,
            stabilized,
            Some(lifecycle_state),
            Some(LifecycleMetrics::default()),
            architecture_hash,
            architecture,
            result,
        );
    }

    pub fn record_experience_with_lifecycle(
        &mut self,
        semantic_graph: MeaningGraph,
        knowledge_graph: Option<KnowledgeGraph>,
        validation: Option<ValidationScore>,
        inferred_knowledge: Option<KnowledgeGraph>,
        stabilized_knowledge: Option<KnowledgeGraph>,
        lifecycle_state: Option<KnowledgeLifecycleState>,
        lifecycle_metrics: Option<LifecycleMetrics>,
        architecture_hash: u64,
        architecture: ArchitectureState,
        result: EvaluationResult,
    ) {
        let problem_id = ProblemId(self.problems.len() as u64 + 1);
        let knowledge_id = knowledge_graph
            .as_ref()
            .map(|_| KnowledgeId(self.knowledges.len() as u64 + 1));
        let inferred_knowledge_id = inferred_knowledge
            .as_ref()
            .map(|_| InferredKnowledgeId(self.inferred_knowledges.len() as u64 + 1));
        let stabilized_knowledge_id = stabilized_knowledge
            .as_ref()
            .map(|_| StabilizedKnowledgeId(self.stabilized_knowledges.len() as u64 + 1));
        let lifecycle_state_id = lifecycle_state
            .as_ref()
            .map(|_| LifecycleStateId(self.lifecycle_states.len() as u64 + 1));
        let lifecycle_metrics_id = lifecycle_metrics
            .as_ref()
            .map(|_| LifecycleMetricsId(self.lifecycle_metrics.len() as u64 + 1));
        let architecture_id = ArchitectureId(self.architectures.len() as u64 + 1);
        let evaluation_id = EvaluationId(self.evaluations.len() as u64 + 1);
        self.problems.push(ProblemNode {
            problem_id,
            semantic_graph,
        });
        if let Some(graph) = knowledge_graph {
            self.knowledges.push(KnowledgeNode {
                knowledge_id: knowledge_id.expect("knowledge id"),
                graph,
                validation: validation.unwrap_or_default(),
            });
        }
        if let Some(graph) = inferred_knowledge {
            self.inferred_knowledges.push(InferredKnowledgeNode {
                inferred_knowledge_id: inferred_knowledge_id.expect("inferred knowledge id"),
                graph,
            });
        }
        if let Some(graph) = stabilized_knowledge {
            self.stabilized_knowledges.push(StabilizedKnowledgeNode {
                stabilized_knowledge_id: stabilized_knowledge_id.expect("stabilized knowledge id"),
                graph,
            });
        }
        if let Some(state) = lifecycle_state {
            self.lifecycle_states.push(LifecycleStateNode {
                lifecycle_state_id: lifecycle_state_id.expect("lifecycle state id"),
                state,
            });
        }
        if let Some(metrics) = lifecycle_metrics {
            self.lifecycle_metrics.push(LifecycleMetricsNode {
                lifecycle_metrics_id: lifecycle_metrics_id.expect("lifecycle metrics id"),
                metrics,
            });
        }
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
            knowledge_id,
            inferred_knowledge_id,
            stabilized_knowledge_id,
            lifecycle_state_id,
            lifecycle_metrics_id,
            architecture_id,
            evaluation_id,
        });
    }
}
