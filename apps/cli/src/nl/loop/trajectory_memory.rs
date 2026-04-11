use super::state::{PatchStrategy, RepairTrajectory};

pub trait TrajectoryStore {
    fn record(&mut self, trajectory: RepairTrajectory);
    fn recall(
        &self,
        failure_signature: &str,
        target_shape: &str,
    ) -> Option<&RepairTrajectory>;
}

#[derive(Clone, Debug, Default)]
pub struct InMemoryTrajectoryStore {
    trajectories: Vec<RepairTrajectory>,
}

impl InMemoryTrajectoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn recall_strategy(
        &self,
        failure_signature: &str,
        target_shape: &str,
    ) -> Option<PatchStrategy> {
        self.recall(failure_signature, target_shape)
            .map(|trajectory| trajectory.patch_strategy)
    }

    pub fn recall_strategy_above_confidence(
        &self,
        failure_signature: &str,
        target_shape: &str,
        min_confidence: f32,
    ) -> Option<PatchStrategy> {
        self.recall(failure_signature, target_shape)
            .filter(|trajectory| trajectory.recall_confidence >= min_confidence)
            .map(|trajectory| trajectory.patch_strategy)
    }
}

impl TrajectoryStore for InMemoryTrajectoryStore {
    fn record(&mut self, trajectory: RepairTrajectory) {
        self.trajectories.push(trajectory);
    }

    fn recall(
        &self,
        failure_signature: &str,
        target_shape: &str,
    ) -> Option<&RepairTrajectory> {
        self.trajectories
            .iter()
            .filter(|trajectory| trajectory.converged)
            .find(|trajectory| {
                trajectory.failure_signature == failure_signature
                    && trajectory.target_shape == target_shape
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recall_prefers_successful_trajectory() {
        let mut store = InMemoryTrajectoryStore::new();
        store.record(RepairTrajectory {
            failure_signature: "E0432".to_string(),
            patch_strategy: PatchStrategy::ImportRebind,
            target_shape: "crate::module".to_string(),
            converged: true,
            recall_confidence: 0.95,
        });

        assert_eq!(
            store.recall_strategy("E0432", "crate::module"),
            Some(PatchStrategy::ImportRebind)
        );
    }

    #[test]
    fn recall_respects_confidence_floor() {
        let mut store = InMemoryTrajectoryStore::new();
        store.record(RepairTrajectory {
            failure_signature: "E0502".to_string(),
            patch_strategy: PatchStrategy::BorrowScopeShrink,
            target_shape: "crate::module".to_string(),
            converged: true,
            recall_confidence: 0.75,
        });
        assert_eq!(
            store.recall_strategy_above_confidence("E0502", "crate::module", 0.8),
            None
        );
    }
}
