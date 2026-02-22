use std::cmp::Ordering;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use memory_store::{Codec, FileStore, InMemoryStore, Store};

pub const EMBEDDING_DIM: usize = 384;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LangId(u64);

impl LangId {
    pub fn value(self) -> u64 {
        self.0
    }
}

impl Codec for LangId {
    fn encode(&self) -> Vec<u8> {
        self.0.to_le_bytes().to_vec()
    }

    fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != 8 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid LangId"));
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(bytes);
        Ok(Self(u64::from_le_bytes(buf)))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct LanguageUnit {
    pub id: LangId,
    pub embedding: Vec<f32>,
    pub raw_text: String,
    pub timestamp: u64,
}

impl Codec for LanguageUnit {
    fn encode(&self) -> Vec<u8> {
        let mut out =
            Vec::with_capacity(8 + 4 + self.embedding.len() * 4 + 8 + 4 + self.raw_text.len());
        out.extend_from_slice(&self.id.0.to_le_bytes());
        out.extend_from_slice(&(self.embedding.len() as u32).to_le_bytes());
        for v in &self.embedding {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.timestamp.to_le_bytes());
        out.extend_from_slice(&(self.raw_text.len() as u32).to_le_bytes());
        out.extend_from_slice(self.raw_text.as_bytes());
        out
    }

    fn decode(bytes: &[u8]) -> io::Result<Self> {
        let mut idx = 0usize;
        let id = read_u64(bytes, &mut idx)?;
        let emb_len = read_u32(bytes, &mut idx)? as usize;
        if emb_len != EMBEDDING_DIM {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "embedding dimension mismatch",
            ));
        }
        let mut embedding = Vec::with_capacity(emb_len);
        for _ in 0..emb_len {
            embedding.push(read_f32(bytes, &mut idx)?);
        }
        let timestamp = read_u64(bytes, &mut idx)?;
        let text_len = read_u32(bytes, &mut idx)? as usize;
        let end = idx.saturating_add(text_len);
        if end > bytes.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "raw_text"));
        }
        let raw_text = String::from_utf8(bytes[idx..end].to_vec())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        Ok(Self {
            id: LangId(id),
            embedding,
            raw_text,
            timestamp,
        })
    }
}

pub struct LanguageDhm<S>
where
    S: Store<LangId, LanguageUnit>,
{
    store: S,
    next_id: u64,
}

impl<S> LanguageDhm<S>
where
    S: Store<LangId, LanguageUnit>,
{
    pub(crate) fn new(store: S) -> io::Result<Self> {
        let next_id = store
            .entries()?
            .into_iter()
            .map(|(id, _)| id.0)
            .max()
            .map(|v| v.saturating_add(1))
            .unwrap_or(1);
        Ok(Self { store, next_id })
    }

    pub fn insert(&mut self, text: &str, embedding: Vec<f32>) -> io::Result<LangId> {
        if embedding.len() != EMBEDDING_DIM {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "embedding length must be EMBEDDING_DIM",
            ));
        }
        let id = LangId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);

        let unit = LanguageUnit {
            id,
            embedding: normalize_l2(&embedding),
            raw_text: text.to_string(),
            timestamp: now_ts(),
        };
        self.store.put(id, unit)?;
        Ok(id)
    }

    pub fn recall(&self, query_embedding: &[f32], top_k: usize) -> Vec<(LangId, f32)> {
        if top_k == 0 || query_embedding.len() != EMBEDDING_DIM {
            return Vec::new();
        }

        let query = normalize_l2(query_embedding);
        let mut scored = self
            .store
            .entries()
            .unwrap_or_default()
            .into_iter()
            .map(|(id, unit)| (id, resonance(&query, &unit.embedding)))
            .collect::<Vec<_>>();

        scored.sort_by(|(_, ls), (_, rs)| rs.partial_cmp(ls).unwrap_or(Ordering::Equal));
        scored.truncate(top_k);
        scored
    }

    pub fn get(&self, id: LangId) -> Option<LanguageUnit> {
        self.store.get(&id).unwrap_or(None)
    }
}

impl LanguageDhm<InMemoryStore<LangId, LanguageUnit>> {
    pub fn in_memory() -> io::Result<Self> {
        Self::new(InMemoryStore::new())
    }
}

impl LanguageDhm<FileStore<LangId, LanguageUnit>> {
    pub fn file(path: impl AsRef<Path>) -> io::Result<Self> {
        let store = FileStore::open(path)?;
        Self::new(store)
    }
}

pub fn resonance(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len().min(b.len());
    let mut dot = 0.0f32;
    for i in 0..n {
        dot += a[i] * b[i];
    }
    dot.abs()
}

pub fn interfere(a: &[f32], b: &[f32]) -> Vec<f32> {
    let n = a.len().min(b.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(a[i] * b[i]);
    }
    out
}

fn normalize_l2(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm <= f32::EPSILON {
        return vec![0.0; v.len()];
    }
    v.iter().map(|x| x / norm).collect()
}

fn now_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_u32(raw: &[u8], idx: &mut usize) -> io::Result<u32> {
    if idx.saturating_add(4) > raw.len() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "u32"));
    }
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&raw[*idx..*idx + 4]);
    *idx += 4;
    Ok(u32::from_le_bytes(buf))
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

fn read_f32(raw: &[u8], idx: &mut usize) -> io::Result<f32> {
    if idx.saturating_add(4) > raw.len() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "f32"));
    }
    let mut buf = [0u8; 4];
    buf.copy_from_slice(&raw[*idx..*idx + 4]);
    *idx += 4;
    Ok(f32::from_le_bytes(buf))
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{EMBEDDING_DIM, LanguageDhm, interfere, resonance};

    fn vec_with(value: f32) -> Vec<f32> {
        vec![value; EMBEDDING_DIM]
    }

    #[test]
    fn normalization_test() {
        let mut dhm = LanguageDhm::in_memory().expect("in-memory");
        let id = dhm.insert("x", vec_with(2.0)).expect("insert");
        let unit = dhm.get(id).expect("unit");
        let norm = unit.embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4);
    }

    #[test]
    fn resonance_monotonicity_test() {
        let a = vec_with(1.0);
        let b = vec_with(1.0);
        let c = vec_with(0.1);
        assert!(resonance(&a, &b) >= resonance(&a, &c));
    }

    #[test]
    fn recall_accuracy_test() {
        let mut dhm = LanguageDhm::in_memory().expect("in-memory");
        let mut near_a = vec![0.0; EMBEDDING_DIM];
        near_a[0] = 1.0;
        let mut near_b = vec![0.0; EMBEDDING_DIM];
        near_b[1] = 1.0;

        let a_id = dhm.insert("A", near_a.clone()).expect("insert a");
        let _ = dhm.insert("B", near_b).expect("insert b");

        let out = dhm.recall(&near_a, 1);
        assert_eq!(out.first().map(|(id, _)| *id), Some(a_id));
    }

    #[test]
    fn restart_consistency_test() {
        let path = std::env::temp_dir().join(format!(
            "language_dhm_restart_{}.bin",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        {
            let mut dhm = LanguageDhm::file(&path).expect("open write");
            let mut a = vec![0.0; EMBEDDING_DIM];
            a[3] = 1.0;
            let _ = dhm.insert("persist", a).expect("insert persist");
        }
        {
            let dhm = LanguageDhm::file(&path).expect("open read");
            let mut q = vec![0.0; EMBEDDING_DIM];
            q[3] = 1.0;
            let out = dhm.recall(&q, 1);
            assert_eq!(out.len(), 1);
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn interfere_hadamard_test() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![4.0, 5.0, 6.0];
        assert_eq!(interfere(&a, &b), vec![4.0, 10.0, 18.0]);
    }
}
