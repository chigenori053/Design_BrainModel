use design_domain::{
    Architecture, Constraint, Dependency, DependencyKind, DesignUnit, DesignUnitId, Layer,
    StructureUnit,
};
use memory_space_core::RecallResult;

#[derive(Debug, Clone, PartialEq)]
pub struct EvaluationVector {
    pub structural_quality: f64,
    pub dependency_quality: f64,
    pub constraint_satisfaction: f64,
    pub complexity: f64,
    pub simulation_quality: f64,
}

impl EvaluationVector {
    pub fn total(&self) -> f64 {
        let reward = self.structural_quality
            + self.dependency_quality
            + self.constraint_satisfaction
            + (1.0 - self.complexity)
            + self.simulation_quality;
        (reward / 5.0).clamp(0.0, 1.0)
    }

    pub fn objectives(&self) -> [f64; 5] {
        [
            self.structural_quality,
            self.dependency_quality,
            self.constraint_satisfaction,
            1.0 - self.complexity,
            self.simulation_quality,
        ]
    }
}

impl Default for EvaluationVector {
    fn default() -> Self {
        Self {
            structural_quality: 0.0,
            dependency_quality: 0.0,
            constraint_satisfaction: 1.0,
            complexity: 0.0,
            simulation_quality: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SystemModelMetrics {
    pub dependency_cycles: usize,
    pub module_coupling: f64,
    pub layering_score: f64,
    pub call_edges: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MathModelMetrics {
    pub algebraic_score: f64,
    pub logic_score: f64,
    pub constraint_solver_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeometryModelMetrics {
    pub graph_layout_score: f64,
    pub layout_balance_score: f64,
    pub spatial_constraint_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionModelMetrics {
    pub runtime_complexity: f64,
    pub memory_usage: f64,
    pub dependency_cost: f64,
    pub latency_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SimulationResult {
    pub performance_score: f64,
    pub correctness_score: f64,
    pub constraint_score: f64,
    pub confidence_score: f64,
    pub system: SystemModelMetrics,
    pub math: MathModelMetrics,
    pub geometry: GeometryModelMetrics,
    pub execution: ExecutionModelMetrics,
}

impl SimulationResult {
    pub fn total(&self) -> f64 {
        ((self.performance_score
            + self.correctness_score
            + self.constraint_score
            + self.confidence_score)
            / 4.0)
            .clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    AddDesignUnit { name: String, layer: Layer },
    RemoveDesignUnit,
    ConnectDependency { from: u64, to: u64 },
    SplitStructure,
    MergeStructure,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorldState {
    pub state_id: u64,
    pub architecture: Architecture,
    pub constraints: Vec<Constraint>,
    pub evaluation: EvaluationVector,
    pub simulation: Option<SimulationResult>,
    pub score: f64,
    pub depth: usize,
    pub history: Vec<Action>,
    pub features: Vec<f64>,
}

impl WorldState {
    pub fn new(state_id: u64, features: Vec<f64>) -> Self {
        let architecture = architecture_from_features(&features);
        let evaluation = evaluate_architecture(&architecture, &[]);
        Self {
            state_id,
            architecture,
            constraints: Vec::new(),
            score: evaluation.total(),
            simulation: None,
            depth: 0,
            history: Vec::new(),
            features,
            evaluation,
        }
    }

    pub fn from_architecture(
        state_id: u64,
        architecture: Architecture,
        constraints: Vec<Constraint>,
    ) -> Self {
        let evaluation = evaluate_architecture(&architecture, &constraints);
        let features = features_from_architecture(&architecture, &evaluation);
        Self {
            state_id,
            architecture,
            constraints,
            evaluation: evaluation.clone(),
            simulation: None,
            score: evaluation.total(),
            depth: 0,
            history: Vec::new(),
            features,
        }
    }

    pub fn with_depth(mut self, depth: usize) -> Self {
        self.depth = depth;
        self
    }

    pub fn apply_action(&self, action: &Action, next_state_id: u64) -> Self {
        let mut next = self.clone();
        next.state_id = next_state_id;
        next.depth = self.depth + 1;
        next.history.push(action.clone());

        match action {
            Action::AddDesignUnit { name, layer } => {
                let next_id = next.architecture.design_unit_count() as u64 + 1;
                next.architecture
                    .add_design_unit(DesignUnit::with_layer(next_id, name.clone(), *layer));
            }
            Action::RemoveDesignUnit => {
                next.architecture.remove_design_unit();
            }
            Action::ConnectDependency { from, to } => {
                let edge = Dependency {
                    from: DesignUnitId(*from),
                    to: DesignUnitId(*to),
                    kind: DependencyKind::Calls,
                };
                if !next.architecture.dependencies.contains(&edge) {
                    next.architecture.graph.edges.push((*from, *to));
                    next.architecture.dependencies.push(edge);
                }
            }
            Action::SplitStructure => {
                next.architecture.ensure_seeded();
                let class = &mut next.architecture.classes[0];
                if let Some(source) = class.structures.first_mut() {
                    if source.design_units.len() > 1 {
                        let split_from = source.design_units.split_off(source.design_units.len() / 2);
                        let mut structure =
                            StructureUnit::new(class.structures.len() as u64 + 1, "split_2");
                        structure.design_units = split_from;
                        class.structures.push(structure);
                    }
                }
            }
            Action::MergeStructure => {
                next.architecture.ensure_seeded();
                let class = &mut next.architecture.classes[0];
                if class.structures.len() >= 2 {
                    let mut merged = class.structures.remove(1);
                    class.structures[0].design_units.append(&mut merged.design_units);
                }
            }
        }

        next.evaluation = evaluate_architecture(&next.architecture, &next.constraints);
        next.simulation = None;
        next.score = next.evaluation.total();
        next.features = features_from_architecture(&next.architecture, &next.evaluation);
        next
    }

    pub fn recall_seed(&self, recall: &RecallResult) -> Option<Self> {
        let candidate = recall.candidates.first()?;
        let mut seeded = WorldState::new(self.state_id, candidate.feature_vector.clone());
        seeded.constraints = self.constraints.clone();
        seeded.evaluation = evaluate_architecture(&seeded.architecture, &seeded.constraints);
        seeded.simulation = None;
        seeded.score = (seeded.evaluation.total() + candidate.relevance_score * 0.2).clamp(0.0, 1.0);
        seeded.features = features_from_architecture(&seeded.architecture, &seeded.evaluation);
        Some(seeded)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Hypothesis {
    pub hypothesis_id: u64,
    pub predicted_state: WorldState,
    pub score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConsistencyScore {
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldModelError {
    EmptyState,
    StateIdMismatch,
}

impl std::fmt::Display for WorldModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyState => write!(f, "state must contain at least one feature"),
            Self::StateIdMismatch => write!(f, "predicted state must preserve state id"),
        }
    }
}

impl std::error::Error for WorldModelError {}

pub type WorldModelResult<T> = Result<T, WorldModelError>;

pub trait WorldModel {
    fn transition(
        &self,
        current: &WorldState,
        hypothesis: &Hypothesis,
    ) -> WorldModelResult<WorldState>;
}

pub trait HypothesisGenerator {
    fn generate(
        &self,
        current: &WorldState,
        recall: Option<&RecallResult>,
    ) -> WorldModelResult<Vec<Hypothesis>>;
}

pub trait ConsistencyEvaluator {
    fn evaluate(
        &self,
        current: &WorldState,
        predicted: &WorldState,
    ) -> WorldModelResult<ConsistencyScore>;
}

#[derive(Debug, Clone, Default)]
pub struct DeterministicWorldModel;

impl WorldModel for DeterministicWorldModel {
    fn transition(
        &self,
        current: &WorldState,
        hypothesis: &Hypothesis,
    ) -> WorldModelResult<WorldState> {
        if current.features.is_empty() || hypothesis.predicted_state.features.is_empty() {
            return Err(WorldModelError::EmptyState);
        }
        if current.state_id != hypothesis.predicted_state.state_id {
            return Err(WorldModelError::StateIdMismatch);
        }

        Ok(hypothesis.predicted_state.clone())
    }
}

#[derive(Debug, Clone, Default)]
pub struct SimpleHypothesisGenerator;

impl HypothesisGenerator for SimpleHypothesisGenerator {
    fn generate(
        &self,
        current: &WorldState,
        recall: Option<&RecallResult>,
    ) -> WorldModelResult<Vec<Hypothesis>> {
        if current.features.is_empty() {
            return Err(WorldModelError::EmptyState);
        }

        let predicted = recall
            .and_then(|result| current.recall_seed(result))
            .unwrap_or_else(|| {
                let features = current.features.iter().map(|value| value + 1.0).collect();
                WorldState::new(current.state_id, features)
            });
        let score = recall
            .and_then(|result| result.candidates.first())
            .map(|candidate| candidate.relevance_score)
            .unwrap_or(0.5);

        Ok(vec![Hypothesis {
            hypothesis_id: current.state_id.saturating_mul(10).saturating_add(1),
            predicted_state: predicted,
            score,
        }])
    }
}

#[derive(Debug, Clone, Default)]
pub struct DeltaConsistencyEvaluator;

impl ConsistencyEvaluator for DeltaConsistencyEvaluator {
    fn evaluate(
        &self,
        current: &WorldState,
        predicted: &WorldState,
    ) -> WorldModelResult<ConsistencyScore> {
        if current.features.is_empty() || predicted.features.is_empty() {
            return Err(WorldModelError::EmptyState);
        }
        if current.state_id != predicted.state_id {
            return Err(WorldModelError::StateIdMismatch);
        }

        let dims = current.features.len().min(predicted.features.len());
        if dims == 0 {
            return Err(WorldModelError::EmptyState);
        }

        let total_delta = current
            .features
            .iter()
            .zip(predicted.features.iter())
            .take(dims)
            .map(|(lhs, rhs)| (rhs - lhs).abs())
            .sum::<f64>();
        let mean_delta = total_delta / dims as f64;

        Ok(ConsistencyScore {
            value: (1.0 / (1.0 + mean_delta)).clamp(0.0, 1.0),
        })
    }
}

fn architecture_from_features(features: &[f64]) -> Architecture {
    let mut architecture = Architecture::seeded();
    let units = features
        .first()
        .copied()
        .unwrap_or(0.0)
        .round()
        .clamp(0.0, 8.0) as usize;
    for index in 0..units {
        architecture.add_design_unit(DesignUnit::new(index as u64 + 1, format!("design_unit_{index}")));
    }
    if features.get(1).copied().unwrap_or_default() > 0.5 && units >= 2 {
        architecture.dependencies.push(Dependency {
            from: DesignUnitId(1),
            to: DesignUnitId(2),
            kind: DependencyKind::Calls,
        });
        architecture.graph.edges.push((1, 2));
    }
    architecture
}

fn features_from_architecture(
    architecture: &Architecture,
    evaluation: &EvaluationVector,
) -> Vec<f64> {
    vec![
        architecture.design_unit_count() as f64,
        architecture.dependencies.len() as f64,
        evaluation.structural_quality,
        evaluation.dependency_quality,
        evaluation.simulation_quality,
    ]
}

pub fn evaluate_architecture(
    architecture: &Architecture,
    constraints: &[Constraint],
) -> EvaluationVector {
    let units = architecture.design_unit_count() as f64;
    let structures = architecture.structure_count().max(1) as f64;
    let dependencies = architecture.dependencies.len() as f64;
    let structural_quality = if units == 0.0 {
        0.2
    } else {
        (units / (structures * 2.0)).min(1.0)
    };
    let dependency_quality = if units <= 1.0 {
        0.6
    } else {
        (dependencies / units).min(1.0)
    };
    let satisfied = constraints
        .iter()
        .filter(|constraint| constraint.satisfied_by(architecture))
        .count();
    let constraint_satisfaction = if constraints.is_empty() {
        1.0
    } else {
        satisfied as f64 / constraints.len() as f64
    };
    let complexity = ((units + dependencies + structures - 1.0) / 12.0).clamp(0.0, 1.0);

    EvaluationVector {
        structural_quality,
        dependency_quality,
        constraint_satisfaction,
        complexity,
        simulation_quality: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory_space_core::RecallCandidate;

    #[test]
    fn deterministic_world_model_returns_predicted_state() {
        let model = DeterministicWorldModel;
        let state = WorldState::new(1, vec![0.0, 0.0]);
        let hypothesis = Hypothesis {
            hypothesis_id: 11,
            predicted_state: WorldState::new(1, vec![1.0, 1.0]),
            score: 0.5,
        };

        let transitioned = model.transition(&state, &hypothesis).unwrap();

        assert_eq!(transitioned.features, vec![1.0, 1.0]);
    }

    #[test]
    fn consistency_score_is_deterministic() {
        let evaluator = DeltaConsistencyEvaluator;
        let current = WorldState::new(1, vec![0.0, 0.0]);
        let predicted = WorldState::new(1, vec![1.0, 1.0]);

        let score = evaluator.evaluate(&current, &predicted).unwrap();

        assert_eq!(score.value, 0.5);
    }

    #[test]
    fn recall_result_seeds_hypothesis_generation() {
        let generator = SimpleHypothesisGenerator;
        let current = WorldState::new(1, vec![0.0, 0.0]);
        let recall = RecallResult {
            candidates: vec![RecallCandidate {
                memory_id: 7,
                feature_vector: vec![1.0, 0.0],
                relevance_score: 0.9,
            }],
        };

        let hypotheses = generator.generate(&current, Some(&recall)).unwrap();

        assert_eq!(hypotheses[0].predicted_state.features, vec![1.0, 0.0, 0.5, 0.6, 0.0]);
        assert_eq!(hypotheses[0].score, 0.9);
    }

    #[test]
    fn world_state_tracks_action_history() {
        let state = WorldState::from_architecture(1, Architecture::seeded(), Vec::new());

        let next = state.apply_action(
            &Action::AddDesignUnit {
                name: "ValidateRequest".into(),
                layer: Layer::Service,
            },
            2,
        );

        assert_eq!(next.depth, 1);
        assert_eq!(next.history.len(), 1);
        assert_eq!(next.architecture.design_unit_count(), 1);
    }
}
