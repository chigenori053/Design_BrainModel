use core_types::{
    CanonicalResolution, CanonicalReuseInput, CanonicalReuseRef, CanonicalReuseResolver,
    ReuseDecision, ReuseDomain, ReuseScope,
};

use crate::{DesignNode, MemoryEntry, MemoryStore, StructuralGraph};

#[derive(Clone, Debug, PartialEq)]
pub enum CanonicalMemoryInsertResult {
    Stored {
        canonical_ref: CanonicalReuseRef,
        entry: MemoryEntry,
    },
    Reused {
        canonical_ref: CanonicalReuseRef,
    },
}

pub struct CanonicalMemoryStore<S> {
    store: S,
    resolver: CanonicalReuseResolver,
    scope: ReuseScope,
}

impl<S> CanonicalMemoryStore<S> {
    pub fn new(store: S, resolver: CanonicalReuseResolver, scope: ReuseScope) -> Self {
        Self {
            store,
            resolver,
            scope,
        }
    }

    pub fn resolver(&self) -> &CanonicalReuseResolver {
        &self.resolver
    }

    pub fn resolver_mut(&mut self) -> &mut CanonicalReuseResolver {
        &mut self.resolver
    }

    pub fn into_inner(self) -> (S, CanonicalReuseResolver) {
        (self.store, self.resolver)
    }
}

impl<S: MemoryStore> CanonicalMemoryStore<S> {
    pub fn insert(&mut self, entry: &MemoryEntry) -> std::io::Result<CanonicalMemoryInsertResult> {
        let resolution = self
            .resolver
            .resolve(memory_input(entry, self.scope.clone()));
        if resolution.decision == ReuseDecision::Reuse {
            return Ok(CanonicalMemoryInsertResult::Reused {
                canonical_ref: resolution.canonical_ref,
            });
        }

        self.store.append(entry)?;
        Ok(CanonicalMemoryInsertResult::Stored {
            canonical_ref: resolution.canonical_ref,
            entry: entry.clone(),
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum CanonicalNodeInsertionResult {
    CanonicalNode {
        graph: StructuralGraph,
        resolution: CanonicalResolution,
    },
    Alias {
        graph: StructuralGraph,
        resolution: CanonicalResolution,
    },
}

impl StructuralGraph {
    pub fn with_canonical_node_added(
        &self,
        node: DesignNode,
        resolver: &mut CanonicalReuseResolver,
        scope: ReuseScope,
    ) -> CanonicalNodeInsertionResult {
        let resolution = resolver.resolve(CanonicalReuseInput {
            domain: ReuseDomain::StateGraph,
            source: format!("{:?}:{:?}", node.id, node.attributes),
            semantic_terms: node
                .attributes
                .iter()
                .map(|(key, value)| format!("{key}:{value:?}"))
                .collect(),
            trajectory_terms: vec![format!("{:?}", node.id)],
            scope,
        });

        if resolution.decision == ReuseDecision::Reuse {
            CanonicalNodeInsertionResult::Alias {
                graph: self.clone(),
                resolution,
            }
        } else {
            CanonicalNodeInsertionResult::CanonicalNode {
                graph: self.with_node_added(node),
                resolution,
            }
        }
    }
}

fn memory_input(entry: &MemoryEntry, scope: ReuseScope) -> CanonicalReuseInput {
    let vector_signature = entry
        .vector
        .iter()
        .map(|value| format!("{value:.12}"))
        .collect::<Vec<_>>()
        .join(",");
    CanonicalReuseInput {
        domain: ReuseDomain::HolographicMemory,
        source: format!("{}:{}:{}", entry.depth, entry.timestamp, vector_signature),
        semantic_terms: vec![vector_signature],
        trajectory_terms: vec![entry.depth.to_string(), entry.timestamp.to_string()],
        scope,
    }
}

#[cfg(test)]
mod tests {
    use core_types::{CanonicalReuseResolver, ReuseScope};

    use crate::{
        CanonicalMemoryInsertResult, CanonicalMemoryStore, CanonicalNodeInsertionResult,
        DesignNode, MemoryEntry, StructuralGraph, Uuid, Value, store_adapter::MemoryStore,
    };

    #[derive(Default)]
    struct VecStore {
        entries: Vec<MemoryEntry>,
    }

    impl MemoryStore for VecStore {
        fn append(&self, _entry: &MemoryEntry) -> std::io::Result<()> {
            Ok(())
        }

        fn entries(&self) -> std::io::Result<Vec<MemoryEntry>> {
            Ok(self.entries.clone())
        }

        fn entry_count(&self) -> std::io::Result<u64> {
            Ok(self.entries.len() as u64)
        }
    }

    #[test]
    fn canonical_memory_insert_reuses_duplicate_without_second_store() {
        let store = VecStore::default();
        let mut canonical = CanonicalMemoryStore::new(
            store,
            CanonicalReuseResolver::new(),
            ReuseScope::Domain(core_types::ReuseDomain::HolographicMemory),
        );
        let entry = MemoryEntry {
            id: 1,
            depth: 2,
            timestamp: 3,
            vector: vec![0.1, 0.2],
        };

        let first = canonical.insert(&entry).expect("first insert");
        let second = canonical.insert(&entry).expect("second insert");

        assert!(matches!(first, CanonicalMemoryInsertResult::Stored { .. }));
        assert!(matches!(second, CanonicalMemoryInsertResult::Reused { .. }));
    }

    #[test]
    fn state_graph_duplicate_node_becomes_alias() {
        let mut attrs = std::collections::BTreeMap::new();
        attrs.insert("kind".to_string(), Value::Text("state".to_string()));
        let node = DesignNode::new(Uuid::from_u128(1), "state", attrs);
        let graph = StructuralGraph::default();
        let mut resolver = CanonicalReuseResolver::new();

        let first =
            graph.with_canonical_node_added(node.clone(), &mut resolver, ReuseScope::Global);
        let graph = match first {
            CanonicalNodeInsertionResult::CanonicalNode { graph, .. } => graph,
            CanonicalNodeInsertionResult::Alias { .. } => panic!("first node must be canonical"),
        };
        let second = graph.with_canonical_node_added(node, &mut resolver, ReuseScope::Global);

        assert!(matches!(second, CanonicalNodeInsertionResult::Alias { .. }));
    }
}
