use crate::ir::IRPersistenceStore;

use super::planner::ReplayTimeline;

pub fn attach_replay_context(ir: &IRPersistenceStore, session_id: &str) -> ReplayTimeline {
    ir.export_timeline(session_id).unwrap_or_default()
}
