pub mod exploration;
pub mod graph;
pub mod interference_memory;
pub mod memory_entry;
pub mod node;
pub mod state;
pub mod store_adapter;
pub mod types;

pub use exploration::ExplorationMemory;
pub use graph::StructuralGraph;
pub use interference_memory::{InterferenceMode, MemoryInterferenceTelemetry, MemorySpace};
pub use memory_entry::MemoryEntry;
pub use node::DesignNode;
pub use state::DesignState;
pub use store_adapter::{FileMemoryStore, MemoryStore};
#[allow(deprecated)]
pub use store_adapter::{HolographicVectorStoreAdapter, LegacyMemoryStore, LegacyStoreAdapter};
pub use types::{NodeId, StateId, Uuid, Value};

#[cfg(test)]
mod tests {
    use crate::{
        DesignNode, DesignState, ExplorationMemory, FileMemoryStore, MemoryEntry, MemoryStore,
        StructuralGraph, Value,
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

    #[test]
    fn memory_store_canonical_api_is_public() {
        fn assert_memory_store<T: MemoryStore>() {}
        fn assert_root_memory_store<T: crate::MemoryStore>() {}

        let _store_type: Option<crate::FileMemoryStore> = None;

        assert_memory_store::<FileMemoryStore>();
        assert_root_memory_store::<crate::FileMemoryStore>();
    }

    #[allow(deprecated)]
    #[test]
    fn legacy_aliases_still_compile() {
        fn assert_legacy_memory_store<T: crate::LegacyMemoryStore>() {}

        let _holographic_alias: Option<crate::HolographicVectorStoreAdapter> = None;
        let _legacy_alias: Option<crate::LegacyStoreAdapter> = None;

        assert_legacy_memory_store::<crate::HolographicVectorStoreAdapter>();
        assert_legacy_memory_store::<crate::LegacyStoreAdapter>();
    }

    #[test]
    fn memory_entry_remains_public_after_reexport_removal() {
        let entry: crate::MemoryEntry = MemoryEntry {
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
