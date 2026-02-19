use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use meaning_extractor::{MeaningStructure, NodeId, RelationType, RoleType};
use memory_store::{Codec, FileStore, InMemoryStore, Store};

pub const D_SEM: usize = 384;
pub const D_STRUCT: usize = 384;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConceptId(pub u64);

impl Codec for ConceptId {
    fn encode(&self) -> Vec<u8> {
        self.0.to_le_bytes().to_vec()
    }

    fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != 8 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid ConceptId"));
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(bytes);
        Ok(Self(u64::from_le_bytes(buf)))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConceptUnit {
    pub id: ConceptId,
    pub v: Vec<f32>,
    pub a: f32,
    pub s: Vec<f32>,
    pub timestamp: u64,
}

impl Codec for ConceptUnit {
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.id.0.to_le_bytes());
        out.extend_from_slice(&(self.v.len() as u32).to_le_bytes());
        for x in &self.v {
            out.extend_from_slice(&x.to_le_bytes());
        }
        out.extend_from_slice(&self.a.to_le_bytes());
        out.extend_from_slice(&(self.s.len() as u32).to_le_bytes());
        for x in &self.s {
            out.extend_from_slice(&x.to_le_bytes());
        }
        out.extend_from_slice(&self.timestamp.to_le_bytes());
        out
    }

    fn decode(bytes: &[u8]) -> io::Result<Self> {
        let mut idx = 0usize;
        let id = read_u64(bytes, &mut idx)?;

        let v_len = read_u32(bytes, &mut idx)? as usize;
        let mut v = Vec::with_capacity(v_len);
        for _ in 0..v_len {
            v.push(read_f32(bytes, &mut idx)?);
        }

        let a = read_f32(bytes, &mut idx)?;

        let s_len = read_u32(bytes, &mut idx)? as usize;
        let mut s = Vec::with_capacity(s_len);
        for _ in 0..s_len {
            s.push(read_f32(bytes, &mut idx)?);
        }

        let timestamp = read_u64(bytes, &mut idx)?;

        Ok(Self {
            id: ConceptId(id),
            v,
            a,
            s,
            timestamp,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ResonanceWeights {
    pub gamma1: f32,
    pub gamma2: f32,
    pub gamma3: f32,
}

impl Default for ResonanceWeights {
    fn default() -> Self {
        Self {
            gamma1: 0.5,
            gamma2: 0.3,
            gamma3: 0.2,
        }
    }
}

impl ResonanceWeights {
    pub fn normalized(self) -> Self {
        let sum = (self.gamma1 + self.gamma2 + self.gamma3).max(1e-9);
        Self {
            gamma1: self.gamma1 / sum,
            gamma2: self.gamma2 / sum,
            gamma3: self.gamma3 / sum,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConceptQuery {
    pub v: Vec<f32>,
    pub a: f32,
    pub s: Vec<f32>,
}

impl ConceptQuery {
    pub fn normalized(self) -> Self {
        Self {
            v: normalize_with_dim(&self.v, D_SEM),
            a: self.a.clamp(0.0, 1.0),
            s: normalize_with_dim(&self.s, D_STRUCT),
        }
    }
}

pub struct SemanticDhm<S>
where
    S: Store<ConceptId, ConceptUnit>,
{
    store: S,
    next_id: u64,
    weights: ResonanceWeights,
}

impl<S> SemanticDhm<S>
where
    S: Store<ConceptId, ConceptUnit>,
{
    pub(crate) fn new(store: S, weights: ResonanceWeights) -> io::Result<Self> {
        let next_id = store
            .entries()?
            .into_iter()
            .map(|(id, _)| id.0)
            .max()
            .map(|v| v.saturating_add(1))
            .unwrap_or(1);
        Ok(Self {
            store,
            next_id,
            weights: weights.normalized(),
        })
    }

    pub fn project(&self, m: &MeaningStructure) -> ConceptQuery {
        phi(m)
    }

    pub fn insert_meaning(&mut self, m: &MeaningStructure) -> ConceptId {
        let query = phi(m);
        self.insert_query(&query)
    }

    pub fn insert_query(&mut self, query: &ConceptQuery) -> ConceptId {
        let q = query.clone().normalized();
        let id = ConceptId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);

        let unit = ConceptUnit {
            id,
            v: q.v,
            a: q.a,
            s: q.s,
            timestamp: now_ts(),
        };

        self.store.put(id, unit).expect("failed to store concept");
        id
    }

    pub fn get(&self, id: ConceptId) -> Option<ConceptUnit> {
        self.store.get(&id).unwrap_or(None)
    }

    pub fn recall(&self, query: &ConceptQuery, top_k: usize) -> Vec<(ConceptId, f32)> {
        if top_k == 0 {
            return Vec::new();
        }
        let q = query.clone().normalized();
        let mut scored = self
            .store
            .entries()
            .unwrap_or_default()
            .into_iter()
            .map(|(id, c)| {
                let score = resonance(&q, &c, self.weights);
                (id, score)
            })
            .collect::<Vec<_>>();

        scored.sort_by(|(_, ls), (_, rs)| rs.partial_cmp(ls).unwrap_or(Ordering::Equal));
        scored.truncate(top_k);
        scored
    }

    pub fn weights(&self) -> ResonanceWeights {
        self.weights
    }
}

impl SemanticDhm<InMemoryStore<ConceptId, ConceptUnit>> {
    pub fn in_memory() -> io::Result<Self> {
        Self::new(InMemoryStore::new(), ResonanceWeights::default())
    }
}

impl SemanticDhm<FileStore<ConceptId, ConceptUnit>> {
    pub fn file(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::new(FileStore::open(path)?, ResonanceWeights::default())
    }
}

pub fn phi(m: &MeaningStructure) -> ConceptQuery {
    let mut node_map: BTreeMap<NodeId, Vec<f32>> = BTreeMap::new();

    let mut sem_acc = vec![0.0f32; D_SEM];
    for node in &m.nodes {
        let e = normalize_with_dim(&node.semantic_vector, D_SEM);
        let alpha = role_weight(node.role);
        add_scaled(&mut sem_acc, &e, alpha);
        node_map.insert(node.id, e);
    }

    let v = normalize_with_dim(&sem_acc, D_SEM);

    let mut struct_acc = vec![0.0f32; D_STRUCT];
    for edge in &m.edges {
        let Some(from) = node_map.get(&edge.from) else {
            continue;
        };
        let Some(to) = node_map.get(&edge.to) else {
            continue;
        };
        let f_uv = hadamard(from, to);
        let beta = relation_weight(edge.relation);
        add_scaled(&mut struct_acc, &f_uv, beta);
    }
    let s = normalize_with_dim(&struct_acc, D_STRUCT);

    ConceptQuery {
        v,
        a: m.abstraction_score.clamp(0.0, 1.0),
        s,
    }
}

pub fn resonance(query: &ConceptQuery, c: &ConceptUnit, weights: ResonanceWeights) -> f32 {
    let w = weights.normalized();
    let q = query.clone().normalized();
    let cv = normalize_with_dim(&c.v, D_SEM);
    let cs = normalize_with_dim(&c.s, D_STRUCT);

    w.gamma1 * dot(&q.v, &cv) + w.gamma2 * dot(&q.s, &cs) - w.gamma3 * (q.a - c.a).abs()
}

pub fn energy(query: &ConceptQuery, c: &ConceptUnit, weights: ResonanceWeights) -> f32 {
    -resonance(query, c, weights)
}

pub fn fuse(a: &ConceptUnit, b: &ConceptUnit) -> ConceptQuery {
    let v = normalize_with_dim(&add_vec(&a.v, &b.v), D_SEM);
    let s = normalize_with_dim(&add_vec(&a.s, &b.s), D_STRUCT);
    let abs = ((a.a + b.a) * 0.5).clamp(0.0, 1.0);
    ConceptQuery { v, a: abs, s }
}

pub fn abstract_move(c: &ConceptUnit, delta: f32) -> ConceptQuery {
    ConceptQuery {
        v: normalize_with_dim(&c.v, D_SEM),
        a: (c.a + delta).clamp(0.0, 1.0),
        s: normalize_with_dim(&c.s, D_STRUCT),
    }
}

pub fn repulse(c: &ConceptUnit, conflict: &[f32], lambda: f32) -> ConceptQuery {
    let conf = normalize_with_dim(conflict, D_SEM);
    let mut shifted = normalize_with_dim(&c.v, D_SEM);
    for i in 0..shifted.len().min(conf.len()) {
        shifted[i] -= lambda.max(0.0) * conf[i];
    }
    ConceptQuery {
        v: normalize_with_dim(&shifted, D_SEM),
        a: c.a.clamp(0.0, 1.0),
        s: normalize_with_dim(&c.s, D_STRUCT),
    }
}

pub fn is_stable(prev_r: f32, curr_r: f32, eps: f32) -> bool {
    (curr_r - prev_r).abs() < eps.abs()
}

fn role_weight(role: RoleType) -> f32 {
    match role {
        RoleType::Subject => 1.0,
        RoleType::Action => 1.2,
        RoleType::Object => 1.0,
        RoleType::Modifier => 0.5,
        RoleType::Constraint => 0.7,
        RoleType::Condition => 0.6,
        RoleType::Abstraction => 0.8,
    }
}

fn relation_weight(relation: RelationType) -> f32 {
    match relation {
        RelationType::AgentOf => 1.0,
        RelationType::ActsOn => 1.0,
        RelationType::Modifies => 0.7,
        RelationType::Causes => 0.9,
        RelationType::DependsOn => 0.8,
        RelationType::IsAbstractOf => 0.6,
    }
}

fn hadamard(a: &[f32], b: &[f32]) -> Vec<f32> {
    let n = a.len().min(b.len());
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        out.push(a[i] * b[i]);
    }
    out
}

fn add_scaled(acc: &mut [f32], v: &[f32], scale: f32) {
    for i in 0..acc.len().min(v.len()) {
        acc[i] += v[i] * scale;
    }
}

fn add_vec(a: &[f32], b: &[f32]) -> Vec<f32> {
    let n = a.len().max(b.len());
    let mut out = vec![0.0f32; n];
    for i in 0..n {
        let av = if i < a.len() { a[i] } else { 0.0 };
        let bv = if i < b.len() { b[i] } else { 0.0 };
        out[i] = av + bv;
    }
    out
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    let mut sum = 0.0;
    for i in 0..a.len().min(b.len()) {
        sum += a[i] * b[i];
    }
    sum
}

fn normalize_with_dim(v: &[f32], dim: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; dim];
    let n = v.len().min(dim);
    out[..n].copy_from_slice(&v[..n]);
    let norm = out.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm <= f32::EPSILON {
        return out;
    }
    out.iter_mut().for_each(|x| *x /= norm);
    out
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

    use meaning_extractor::{MeaningEdge, MeaningNode};

    use super::*;

    fn sample_structure() -> MeaningStructure {
        let n1 = MeaningNode {
            id: NodeId(1),
            role: RoleType::Subject,
            token_span: (0, 1),
            semantic_vector: vec![1.0; D_SEM],
        };
        let n2 = MeaningNode {
            id: NodeId(2),
            role: RoleType::Action,
            token_span: (1, 2),
            semantic_vector: vec![0.5; D_SEM],
        };
        let n3 = MeaningNode {
            id: NodeId(3),
            role: RoleType::Object,
            token_span: (2, 3),
            semantic_vector: vec![0.2; D_SEM],
        };
        MeaningStructure {
            root: NodeId(2),
            nodes: vec![n1, n2, n3],
            edges: vec![
                MeaningEdge {
                    from: NodeId(1),
                    to: NodeId(2),
                    relation: RelationType::AgentOf,
                },
                MeaningEdge {
                    from: NodeId(2),
                    to: NodeId(3),
                    relation: RelationType::ActsOn,
                },
            ],
            abstraction_score: 0.4,
        }
    }

    #[test]
    fn projection_normalization_and_abstraction() {
        let q = phi(&sample_structure());
        let nv = q.v.iter().map(|x| x * x).sum::<f32>().sqrt();
        let ns = q.s.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((nv - 1.0).abs() < 1e-4);
        assert!((ns - 1.0).abs() < 1e-4);
        assert!((q.a - 0.4).abs() < 1e-6);
    }

    #[test]
    fn resonance_and_energy() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let m = sample_structure();
        let id = dhm.insert_meaning(&m);
        let c = dhm.get(id).expect("get");
        let q = phi(&m);
        let r = resonance(&q, &c, dhm.weights());
        let e = energy(&q, &c, dhm.weights());
        assert!(r > 0.0);
        assert!((e + r).abs() < 1e-6);
    }

    #[test]
    fn recall_selects_max_resonance() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let m1 = sample_structure();
        let id1 = dhm.insert_meaning(&m1);

        let mut m2 = sample_structure();
        m2.abstraction_score = 0.95;
        let _ = dhm.insert_meaning(&m2);

        let out = dhm.recall(&phi(&m1), 1);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, id1);
    }

    #[test]
    fn fusion_abstract_and_repulse_work() {
        let mut dhm = SemanticDhm::in_memory().expect("mem");
        let m1 = sample_structure();
        let id1 = dhm.insert_meaning(&m1);
        let mut m2 = sample_structure();
        m2.abstraction_score = 0.8;
        let id2 = dhm.insert_meaning(&m2);

        let c1 = dhm.get(id1).expect("c1");
        let c2 = dhm.get(id2).expect("c2");

        let fused = fuse(&c1, &c2);
        assert!((0.0..=1.0).contains(&fused.a));

        let moved = abstract_move(&c1, 0.3);
        assert!(moved.a >= c1.a);

        let rep = repulse(&c1, &vec![1.0; D_SEM], 0.1);
        let norm = rep.v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-4 || norm == 0.0);
    }

    #[test]
    fn restart_persistence() {
        let path = std::env::temp_dir().join(format!(
            "semantic_dhm_restart_{}.bin",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        {
            let mut dhm = SemanticDhm::file(&path).expect("open write");
            let _ = dhm.insert_meaning(&sample_structure());
        }
        {
            let dhm = SemanticDhm::file(&path).expect("open read");
            let out = dhm.recall(&phi(&sample_structure()), 1);
            assert_eq!(out.len(), 1);
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn stability_condition() {
        assert!(is_stable(0.5001, 0.50015, 0.001));
        assert!(!is_stable(0.5, 0.8, 0.001));
    }
}
