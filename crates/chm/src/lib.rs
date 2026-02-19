use std::collections::BTreeMap;
use std::io;

use memory_space::Uuid;
use memory_store::{Codec, FileStore, InMemoryStore, Store};

pub type RuleId = Uuid;

#[derive(Clone, Debug, PartialEq)]
pub struct CausalEdge {
    pub from_rule: RuleId,
    pub to_rule: RuleId,
    pub strength: f64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ChmKey(pub RuleId);

impl Codec for ChmKey {
    fn encode(&self) -> Vec<u8> {
        self.0.as_u128().to_le_bytes().to_vec()
    }

    fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != 16 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid chm key",
            ));
        }
        let mut buf = [0u8; 16];
        buf.copy_from_slice(bytes);
        Ok(Self(Uuid::from_u128(u128::from_le_bytes(buf))))
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ChmEdgeList(pub Vec<CausalEdge>);

impl Codec for ChmEdgeList {
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(8 + self.0.len() * 40);
        out.extend_from_slice(&(self.0.len() as u64).to_le_bytes());
        for edge in &self.0 {
            out.extend_from_slice(&edge.from_rule.as_u128().to_le_bytes());
            out.extend_from_slice(&edge.to_rule.as_u128().to_le_bytes());
            out.extend_from_slice(&edge.strength.to_le_bytes());
        }
        out
    }

    fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() < 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid chm edge list",
            ));
        }
        let mut idx = 0usize;
        let count = read_u64(bytes, &mut idx)? as usize;
        let mut edges = Vec::with_capacity(count);
        for _ in 0..count {
            let from = read_u128(bytes, &mut idx)?;
            let to = read_u128(bytes, &mut idx)?;
            let strength = read_f64(bytes, &mut idx)?;
            edges.push(CausalEdge {
                from_rule: Uuid::from_u128(from),
                to_rule: Uuid::from_u128(to),
                strength,
            });
        }
        Ok(Self(edges))
    }
}

#[derive(Debug)]
pub struct ChmStore<S>
where
    S: Store<ChmKey, ChmEdgeList>,
{
    inner: S,
}

impl<S> ChmStore<S>
where
    S: Store<ChmKey, ChmEdgeList>,
{
    pub fn new(inner: S) -> Self {
        Self { inner }
    }

    pub fn put(&self, key: ChmKey, value: ChmEdgeList) -> io::Result<()> {
        self.inner.put(key, value)
    }

    pub fn get(&self, key: &ChmKey) -> io::Result<Option<ChmEdgeList>> {
        self.inner.get(key)
    }
}

pub type InMemoryChmStore = ChmStore<InMemoryStore<ChmKey, ChmEdgeList>>;
pub type FileChmStore = ChmStore<FileStore<ChmKey, ChmEdgeList>>;

#[derive(Clone, Debug)]
pub struct Chm {
    rule_graph: BTreeMap<RuleId, Vec<CausalEdge>>,
}

impl Chm {
    pub(crate) fn new(rule_graph: BTreeMap<RuleId, Vec<CausalEdge>>) -> Self {
        Self { rule_graph }
    }

    pub fn insert_edge(&mut self, from_rule: RuleId, to_rule: RuleId, strength: f64) {
        if from_rule == to_rule {
            return;
        }

        let clamped = clamp_strength(strength);
        let edges = self.rule_graph.entry(from_rule).or_default();

        if let Some(edge) = edges.iter_mut().find(|edge| edge.to_rule == to_rule) {
            edge.strength = clamped;
            return;
        }

        edges.push(CausalEdge {
            from_rule,
            to_rule,
            strength: clamped,
        });
    }

    pub fn related_rules(&self, rule_id: RuleId) -> Vec<RuleId> {
        self.rule_graph
            .get(&rule_id)
            .map(|edges| edges.iter().map(|edge| edge.to_rule).collect())
            .unwrap_or_default()
    }

    pub fn update_strength(&mut self, from: RuleId, to: RuleId, delta: f64) {
        if from == to {
            return;
        }

        let edges = self.rule_graph.entry(from).or_default();
        if let Some(edge) = edges.iter_mut().find(|edge| edge.to_rule == to) {
            edge.strength = clamp_strength(edge.strength + delta);
            return;
        }

        edges.push(CausalEdge {
            from_rule: from,
            to_rule: to,
            strength: clamp_strength(delta),
        });
    }

    pub fn edge_count(&self) -> usize {
        self.rule_graph.values().map(|v| v.len()).sum::<usize>()
    }
}

impl Default for Chm {
    fn default() -> Self {
        Self::new(BTreeMap::new())
    }
}

fn clamp_strength(value: f64) -> f64 {
    value.clamp(-1.0, 1.0)
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

fn read_u128(raw: &[u8], idx: &mut usize) -> io::Result<u128> {
    if idx.saturating_add(16) > raw.len() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "u128"));
    }
    let mut buf = [0u8; 16];
    buf.copy_from_slice(&raw[*idx..*idx + 16]);
    *idx += 16;
    Ok(u128::from_le_bytes(buf))
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
    use memory_space::Uuid;

    use crate::{Chm, ChmEdgeList, ChmKey, ChmStore, InMemoryChmStore};

    #[test]
    fn edge_insertion() {
        let mut chm = Chm::default();
        let r1 = Uuid::from_u128(1);
        let r2 = Uuid::from_u128(2);

        chm.insert_edge(r1, r2, 0.4);

        let edges = chm.rule_graph.get(&r1).expect("edge list must exist");
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].to_rule, r2);
        assert_eq!(edges[0].strength, 0.4);
    }

    #[test]
    fn strength_update_clamping() {
        let mut chm = Chm::default();
        let r1 = Uuid::from_u128(1);
        let r2 = Uuid::from_u128(2);

        chm.insert_edge(r1, r2, 0.8);
        chm.update_strength(r1, r2, 0.7);

        let edge = &chm.rule_graph.get(&r1).expect("edge list must exist")[0];
        assert_eq!(edge.strength, 1.0);

        chm.update_strength(r1, r2, -2.5);
        let edge = &chm.rule_graph.get(&r1).expect("edge list must exist")[0];
        assert_eq!(edge.strength, -1.0);
    }

    #[test]
    fn related_rule_lookup() {
        let mut chm = Chm::default();
        let r1 = Uuid::from_u128(1);
        let r2 = Uuid::from_u128(2);
        let r3 = Uuid::from_u128(3);

        chm.insert_edge(r1, r2, 0.2);
        chm.insert_edge(r1, r3, -0.1);

        let related = chm.related_rules(r1);
        assert_eq!(related, vec![r2, r3]);
    }

    #[test]
    fn chm_store_roundtrip() {
        let store: InMemoryChmStore = ChmStore::new(memory_store::InMemoryStore::new());
        let key = ChmKey(Uuid::from_u128(5));
        let value = ChmEdgeList::default();
        store.put(key.clone(), value.clone()).expect("put");
        let out = store.get(&key).expect("get");
        assert_eq!(out, Some(value));
    }
}
