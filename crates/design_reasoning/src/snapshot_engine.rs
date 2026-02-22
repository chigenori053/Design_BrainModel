use semantic_dhm::{
    ConceptUnit, MeaningLayerSnapshot, MeaningLayerState, SemanticError, SemanticUnitL1,
    SnapshotDiff, Snapshotable, compare_snapshots,
};

#[derive(Clone, Default)]
pub struct SnapshotEngine;

impl SnapshotEngine {
    pub fn snapshot(
        &self,
        algorithm_version: u32,
        l1_units: Vec<SemanticUnitL1>,
        l2_units: Vec<ConceptUnit>,
    ) -> Result<MeaningLayerSnapshot, SemanticError> {
        Ok(MeaningLayerState {
            algorithm_version,
            l1_units,
            l2_units,
        }
        .snapshot())
    }

    pub fn compare(
        &self,
        a: &MeaningLayerSnapshot,
        b: &MeaningLayerSnapshot,
    ) -> Result<SnapshotDiff, SemanticError> {
        compare_snapshots(a, b)
    }
}
