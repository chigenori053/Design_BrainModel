pub mod exploration;
pub mod graph;
pub mod holographic_store;
pub mod interference_memory;
pub mod node;
pub mod state;
pub mod store_adapter;
pub mod types;

pub use exploration::ExplorationMemory;
pub use graph::StructuralGraph;
#[deprecated(
    since = "0.1.0",
    note = "Use HolographicVectorStoreAdapter and LegacyMemoryStore instead."
)]
pub use holographic_store::HolographicVectorStore;
pub use holographic_store::MemoryEntry;
pub use interference_memory::{InterferenceMode, MemoryInterferenceTelemetry, MemorySpace};
pub use node::DesignNode;
pub use state::DesignState;
pub use store_adapter::{
    HolographicVectorStoreAdapter, HolographicVectorStoreAdapter as LegacyStoreAdapter,
    LegacyMemoryStore,
};
pub use types::{NodeId, StateId, Uuid, Value};

#[cfg(test)]
mod tests {
    use crate::{
        DesignNode, DesignState, ExplorationMemory, HolographicVectorStoreAdapter,
        LegacyMemoryStore, MemoryEntry, StructuralGraph, Value,
    };

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn public_types_are_send_and_sync() {
        assert_send_sync::<Value>();
        assert_send_sync::<DesignNode>();
        assert_send_sync::<StructuralGraph>();
        assert_send_sync::<DesignState>();
        assert_send_sync::<ExplorationMemory>();
    }

    #[allow(deprecated)]
    #[test]
    fn deprecated_holographic_vector_store_reexport_remains_available() {
        fn assert_legacy_store_type<T>() {}

        assert_legacy_store_type::<crate::HolographicVectorStore>();
    }

    #[test]
    fn adapter_is_preferred_public_store_api() {
        fn assert_legacy_memory_store<T: LegacyMemoryStore>() {}

        assert_legacy_memory_store::<HolographicVectorStoreAdapter>();
    }

    #[test]
    fn memory_entry_remains_public_without_deprecation() {
        let entry = MemoryEntry {
            id: 1,
            depth: 2,
            timestamp: 3,
            vector: vec![0.1, 0.2, 0.3, 0.4],
        };

        assert_eq!(entry.id, 1);
        assert_eq!(entry.depth, 2);
        assert_eq!(entry.timestamp, 3);
        assert_eq!(entry.vector.len(), 4);
    }
}

#[cfg(test)]
mod proptest_props {
    use std::collections::BTreeMap;

    use proptest::prelude::*;

    use crate::{DesignNode, StructuralGraph, Uuid};

    fn make_node(id: u64) -> DesignNode {
        DesignNode::new(Uuid::from_u128(id as u128), "node", BTreeMap::new())
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// 同じノードを2回追加しても結果が変わらない（冪等性）
        #[test]
        fn add_node_idempotent(id in 0u64..16u64) {
            let node = make_node(id);
            let g = StructuralGraph::default();
            let g1 = g.with_node_added(node.clone());
            let g2 = g1.with_node_added(node);
            prop_assert_eq!(g1, g2, "adding the same node twice must be idempotent");
        }

        /// 存在しないノードを削除しても変化しない（no-op）
        #[test]
        fn remove_absent_node_is_noop(id in 0u64..16u64) {
            let g = StructuralGraph::default();
            let removed = g.with_node_removed(Uuid::from_u128(id as u128));
            prop_assert_eq!(g, removed, "removing an absent node must be a no-op");
        }

        /// エッジ追加後もノード数は変わらない
        #[test]
        fn add_edge_preserves_node_count(from in 0u64..4u64, to in 0u64..4u64) {
            let nodes: Vec<DesignNode> = (0u64..4).map(make_node).collect();
            let g = nodes.iter().fold(StructuralGraph::default(), |acc, n| {
                acc.with_node_added(n.clone())
            });
            let node_count_before = g.nodes().len();
            let from_id = Uuid::from_u128(from as u128);
            let to_id = Uuid::from_u128(to as u128);
            let g2 = g.with_edge_added(from_id, to_id);
            prop_assert_eq!(
                node_count_before,
                g2.nodes().len(),
                "adding an edge must not change node count"
            );
        }
    }
}
