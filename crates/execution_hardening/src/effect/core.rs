use std::path::PathBuf;

/// A discrete side-effect produced during execution.
///
/// Each variant carries both the new value **and** the previous value needed
/// to roll back the change if the containing transaction is aborted.
///
/// Spec §6: 副作用管理 Atomic化
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// A file was created or overwritten.
    FileWrite {
        path: PathBuf,
        content: Vec<u8>,
        /// `None` when the file did not exist before (create case).
        previous_content: Option<Vec<u8>>,
    },
    /// A file was deleted.
    FileDelete {
        path: PathBuf,
        /// Content before deletion — required for rollback.
        previous_content: Vec<u8>,
    },
    /// A key-value entry in the execution state was created or updated.
    StateSet {
        key: String,
        value: Vec<u8>,
        previous_value: Option<Vec<u8>>,
    },
    /// An in-memory record was created or updated.
    MemoryUpdate {
        id: String,
        data: Vec<u8>,
        previous_data: Option<Vec<u8>>,
    },
}

impl Effect {
    /// A stable, ordering-preserving string key used for checksum computation.
    pub fn stable_key(&self) -> String {
        match self {
            Self::FileWrite { path, .. } => format!("fw:{}", path.display()),
            Self::FileDelete { path, .. } => format!("fd:{}", path.display()),
            Self::StateSet { key, .. } => format!("ss:{key}"),
            Self::MemoryUpdate { id, .. } => format!("mu:{id}"),
        }
    }

    /// Byte payload of the new value (used for checksum).
    pub fn new_value_bytes(&self) -> &[u8] {
        match self {
            Self::FileWrite { content, .. } => content,
            Self::FileDelete { .. } => b"",
            Self::StateSet { value, .. } => value,
            Self::MemoryUpdate { data, .. } => data,
        }
    }
}
