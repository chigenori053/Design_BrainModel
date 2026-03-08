pub mod exploration;
pub mod graph;
pub mod holographic_store;
pub mod interference_memory;
pub mod node;
pub mod state;
pub mod types;

pub use exploration::ExplorationMemory;
pub use graph::StructuralGraph;
pub use holographic_store::{HolographicVectorStore, MemoryEntry};
pub use interference_memory::{InterferenceMode, MemoryInterferenceTelemetry, MemorySpace};
pub use node::DesignNode;
pub use state::DesignState;
pub use types::{NodeId, StateId, Uuid, Value};

#[cfg(test)]
mod tests {
    use crate::{DesignNode, DesignState, ExplorationMemory, StructuralGraph, Value};

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn public_types_are_send_and_sync() {
        assert_send_sync::<Value>();
        assert_send_sync::<DesignNode>();
        assert_send_sync::<StructuralGraph>();
        assert_send_sync::<DesignState>();
        assert_send_sync::<ExplorationMemory>();
    }
}
