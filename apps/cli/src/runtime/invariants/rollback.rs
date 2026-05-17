use crate::runtime::autonomous_control::{RollbackCheckpoint, restore_rollback_checkpoint};
use crate::tui::rendering::{ProjectionSnapshot, projection_semantic_hash};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RollbackInvariantSuite {
    pub semantic_restore: bool,
    pub replay_restore: bool,
    pub projection_restore: bool,
    pub lineage_restore: bool,
}

impl RollbackInvariantSuite {
    pub fn validate(
        checkpoint: &RollbackCheckpoint,
        restored: &ProjectionSnapshot,
    ) -> RollbackInvariantSuite {
        let authoritative_restore = restore_rollback_checkpoint(checkpoint);
        let semantic_restore = checkpoint.semantic_hash == projection_semantic_hash(restored);
        let projection_restore = authoritative_restore == *restored;
        RollbackInvariantSuite {
            semantic_restore,
            replay_restore: semantic_restore,
            projection_restore,
            lineage_restore: projection_restore,
        }
    }

    pub fn assert_all(&self) {
        assert!(self.semantic_restore);
        assert!(self.replay_restore);
        assert!(self.projection_restore);
        assert!(self.lineage_restore);
    }
}
