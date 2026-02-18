use std::io;
use std::path::Path;

use core_types::ObjectiveVector;
use memory_space::{HolographicVectorStore, InterferenceMode, MemoryInterferenceTelemetry, MemorySpace};
use memory_store::{Codec, FileStore, InMemoryStore, Store};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DhmKey(pub u64);

impl Codec for DhmKey {
    fn encode(&self) -> Vec<u8> {
        self.0.to_le_bytes().to_vec()
    }

    fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != 8 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid dhm key"));
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(bytes);
        Ok(Self(u64::from_le_bytes(buf)))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DhmRecord {
    pub depth: usize,
    pub vector: ObjectiveVector,
}

impl Codec for DhmRecord {
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + 32);
        out.extend_from_slice(&(self.depth as u64).to_le_bytes());
        out.extend_from_slice(&self.vector.f_struct.to_le_bytes());
        out.extend_from_slice(&self.vector.f_field.to_le_bytes());
        out.extend_from_slice(&self.vector.f_risk.to_le_bytes());
        out.extend_from_slice(&self.vector.f_shape.to_le_bytes());
        out
    }

    fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != 40 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid dhm record",
            ));
        }
        let mut idx = 0usize;
        let depth = read_u64(bytes, &mut idx)? as usize;
        let vector = ObjectiveVector {
            f_struct: read_f64(bytes, &mut idx)?,
            f_field: read_f64(bytes, &mut idx)?,
            f_risk: read_f64(bytes, &mut idx)?,
            f_shape: read_f64(bytes, &mut idx)?,
        };
        Ok(Self { depth, vector })
    }
}

#[derive(Debug)]
pub struct DhmStore<S>
where
    S: Store<DhmKey, DhmRecord>,
{
    inner: S,
}

impl<S> DhmStore<S>
where
    S: Store<DhmKey, DhmRecord>,
{
    pub fn new(inner: S) -> Self {
        Self { inner }
    }

    pub fn put(&self, key: DhmKey, value: DhmRecord) -> io::Result<()> {
        self.inner.put(key, value)
    }

    pub fn get(&self, key: &DhmKey) -> io::Result<Option<DhmRecord>> {
        self.inner.get(key)
    }
}

pub type InMemoryDhmStore = DhmStore<InMemoryStore<DhmKey, DhmRecord>>;
pub type FileDhmStore = DhmStore<FileStore<DhmKey, DhmRecord>>;

pub struct Dhm {
    memory: MemorySpace,
}

impl Dhm {
    pub fn open(path: impl AsRef<Path>, mode: InterferenceMode) -> io::Result<Self> {
        let store = HolographicVectorStore::open(path, 4)?;
        let lambda = match mode {
            InterferenceMode::Disabled => 0.0,
            InterferenceMode::Contractive => 0.1,
            InterferenceMode::Repulsive => 0.02,
        };
        let memory = MemorySpace::new(store, 0.95, lambda, mode, 256)?;
        Ok(Self { memory })
    }

    pub fn evaluate_with_recall(&mut self, base: &ObjectiveVector, depth: usize) -> ObjectiveVector {
        let adjusted = self.memory.apply_interference(base);
        let _ = self.memory.store(&adjusted, depth);
        adjusted
    }

    pub fn recall_first(&mut self, base: &ObjectiveVector) -> ObjectiveVector {
        self.memory.apply_interference(base)
    }

    pub fn telemetry(&mut self) -> MemoryInterferenceTelemetry {
        self.memory.take_telemetry()
    }
}

fn read_u64(raw: &[u8], idx: &mut usize) -> io::Result<u64> {
    if idx.saturating_add(8) > raw.len() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "u64"));
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&raw[*idx..*idx + 8]);
    *idx += 8;
    Ok(u64::from_le_bytes(buf))
}

fn read_f64(raw: &[u8], idx: &mut usize) -> io::Result<f64> {
    if idx.saturating_add(8) > raw.len() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "f64"));
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&raw[*idx..*idx + 8]);
    *idx += 8;
    Ok(f64::from_le_bytes(buf))
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use memory_space::InterferenceMode;
    use memory_store::{FileStore, InMemoryStore};

    use super::{Dhm, DhmKey, DhmRecord, DhmStore};
    use core_types::ObjectiveVector;

    #[test]
    fn dhm_store_roundtrip() {
        let store = DhmStore::new(InMemoryStore::new());
        let record = DhmRecord {
            depth: 2,
            vector: ObjectiveVector {
                f_struct: 0.9,
                f_field: 0.8,
                f_risk: 0.7,
                f_shape: 0.6,
            },
        };
        store.put(DhmKey(1), record.clone()).expect("put");
        let out = store.get(&DhmKey(1)).expect("get");
        assert_eq!(out, Some(record));
    }

    #[test]
    fn file_store_restart_consistency() {
        let path = std::env::temp_dir().join(format!(
            "dhm_store_restart_{}.bin",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        {
            let inner = FileStore::open(&path).expect("open");
            let store = DhmStore::new(inner);
            store
                .put(
                    DhmKey(11),
                    DhmRecord {
                        depth: 3,
                        vector: ObjectiveVector {
                            f_struct: 0.2,
                            f_field: 0.3,
                            f_risk: 0.4,
                            f_shape: 0.5,
                        },
                    },
                )
                .expect("put");
        }
        {
            let inner = FileStore::open(&path).expect("reopen");
            let store = DhmStore::new(inner);
            let out = store.get(&DhmKey(11)).expect("get");
            assert!(out.is_some());
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn dhm_recall_path_works() {
        let path = std::env::temp_dir().join("dhm_recall_path.bin");
        let mut dhm = Dhm::open(&path, InterferenceMode::Repulsive).expect("open dhm");
        let base = ObjectiveVector {
            f_struct: 0.6,
            f_field: 0.5,
            f_risk: 0.4,
            f_shape: 0.3,
        };
        let _ = dhm.evaluate_with_recall(&base, 1);
        let _ = dhm.recall_first(&base);
        let _ = std::fs::remove_file(path);
    }
}
