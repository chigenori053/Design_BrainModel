use std::sync::Arc;

use crate::graph::StructuralGraph;
use crate::types::StateId;

#[derive(Clone, Debug)]
pub struct DesignState {
    pub id: StateId,
    pub graph: Arc<StructuralGraph>,
    pub profile_snapshot: String,
}

impl DesignState {
    pub fn new(
        id: StateId,
        graph: Arc<StructuralGraph>,
        profile_snapshot: impl Into<String>,
    ) -> Self {
        Self {
            id,
            graph,
            profile_snapshot: profile_snapshot.into(),
        }
    }

    pub fn with_id(
        id: StateId,
        graph: Arc<StructuralGraph>,
        profile_snapshot: impl Into<String>,
    ) -> Self {
        Self::new(id, graph, profile_snapshot)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{DesignState, StructuralGraph, Uuid};

    #[test]
    fn design_state_cloning_preserves_arc_sharing() {
        let state = DesignState::new(
            Uuid::from_u128(7),
            Arc::new(StructuralGraph::default()),
            "snapshot-v1",
        );
        let cloned = state.clone();

        assert!(Arc::ptr_eq(&state.graph, &cloned.graph));
    }
}
