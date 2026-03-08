#[derive(Debug, Clone, PartialEq)]
pub struct WorldState {
    pub state_id: u64,
    pub features: Vec<f64>,
}

impl WorldState {
    pub fn new(state_id: u64, features: Vec<f64>) -> Self {
        Self { state_id, features }
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
            .and_then(|result| result.candidates.first())
            .map(|candidate| WorldState {
                state_id: current.state_id,
                features: candidate.feature_vector.clone(),
            })
            .unwrap_or_else(|| WorldState {
                state_id: current.state_id,
                features: current.features.iter().map(|value| value + 1.0).collect(),
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

#[cfg(test)]
mod tests {
    use super::*;

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
            candidates: vec![memory_space_core::RecallCandidate {
                memory_id: 7,
                feature_vector: vec![9.0, 8.0],
                relevance_score: 0.9,
            }],
        };

        let hypotheses = generator.generate(&current, Some(&recall)).unwrap();

        assert_eq!(hypotheses[0].predicted_state.features, vec![9.0, 8.0]);
        assert_eq!(hypotheses[0].score, 0.9);
    }
}
use memory_space_core::RecallResult;
