use std::collections::HashMap;
use std::sync::Arc;

use crate::tui::rendering::{ProjectionSnapshot, projection_semantic_hash};

#[derive(Debug, Clone, Default)]
pub struct ProjectionCache {
    pub snapshots: HashMap<String, Arc<ProjectionSnapshot>>,
}

impl ProjectionCache {
    pub fn get_or_insert(&mut self, mut snapshot: ProjectionSnapshot) -> Arc<ProjectionSnapshot> {
        let semantic_hash = projection_semantic_hash(&snapshot);
        snapshot.projection_hash.semantic_hash = semantic_hash.clone();
        if let Some(existing) = self.snapshots.get(&semantic_hash) {
            return Arc::clone(existing);
        }
        let snapshot = Arc::new(snapshot);
        self.snapshots.insert(semantic_hash, Arc::clone(&snapshot));
        snapshot
    }

    pub fn get(&self, semantic_hash: &str) -> Option<Arc<ProjectionSnapshot>> {
        self.snapshots.get(semantic_hash).map(Arc::clone)
    }

    pub fn len(&self) -> usize {
        self.snapshots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }
}

pub struct ProjectionInvariantSuite;

impl ProjectionInvariantSuite {
    pub fn assert_projection_reused(
        first: &Arc<ProjectionSnapshot>,
        second: &Arc<ProjectionSnapshot>,
    ) {
        assert!(Arc::ptr_eq(first, second));
    }

    pub fn assert_no_projection_corruption(snapshot: &ProjectionSnapshot) {
        assert_eq!(
            snapshot.projection_hash.semantic_hash,
            projection_semantic_hash(snapshot)
        );
    }
}
