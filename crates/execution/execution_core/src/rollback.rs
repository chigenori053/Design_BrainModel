use crate::types::{AppliedChange, RollbackInfo};

pub struct RollbackEngine;

impl RollbackEngine {
    /// Revert applied changes in LIFO order (last applied = first reverted).
    /// In dry-run mode this is always a no-op (nothing was applied).
    pub fn rollback(changes: Vec<AppliedChange>, dry_run: bool) -> RollbackInfo {
        if dry_run {
            return RollbackInfo::committed(0);
        }
        // Reverse the applied change list to restore original state.
        // In production this would undo file writes and AST transforms.
        let mut reverted = changes;
        reverted.reverse();
        RollbackInfo::rolled_back(reverted)
    }
}
