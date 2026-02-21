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
pub struct L1Id(pub u128);

impl Codec for L1Id {
    fn encode(&self) -> Vec<u8> {
        self.0.to_le_bytes().to_vec()
    }

    fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != 16 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "invalid L1Id"));
        }
        let mut buf = [0u8; 16];
        buf.copy_from_slice(bytes);
        Ok(Self(u128::from_le_bytes(buf)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequirementRole {
    Goal,
    Constraint,
    Optimization,
    Prohibition,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticUnitL1 {
    pub id: L1Id,
    pub role: RequirementRole,
    pub polarity: i8,
    pub abstraction: f32,
    pub vector: Vec<f32>,
    pub source_text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticUnitL1Input {
    pub role: RequirementRole,
    pub polarity: i8,
    pub abstraction: f32,
    pub vector: Vec<f32>,
    pub source_text: String,
}

impl Codec for SemanticUnitL1 {
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.id.0.to_le_bytes());
        out.push(role_to_u8(self.role));
        out.push(self.polarity as u8);
        out.extend_from_slice(&self.abstraction.to_le_bytes());
        out.extend_from_slice(&(self.vector.len() as u32).to_le_bytes());
        for x in &self.vector {
            out.extend_from_slice(&x.to_le_bytes());
        }
        let src = self.source_text.as_bytes();
        out.extend_from_slice(&(src.len() as u32).to_le_bytes());
        out.extend_from_slice(src);
        out
    }

    fn decode(bytes: &[u8]) -> io::Result<Self> {
        let mut idx = 0usize;
        let id = read_u128(bytes, &mut idx)?;
        let role = role_from_u8(read_u8(bytes, &mut idx)?)?;
        let polarity = normalize_polarity_i8(read_u8(bytes, &mut idx)? as i8);
        let abstraction = read_f32(bytes, &mut idx)?.clamp(0.0, 1.0);
        let v_len = read_u32(bytes, &mut idx)? as usize;
        let mut vector = Vec::with_capacity(v_len);
        for _ in 0..v_len {
            vector.push(read_f32(bytes, &mut idx)?);
        }
        let src_len = read_u32(bytes, &mut idx)? as usize;
        if idx.saturating_add(src_len) > bytes.len() {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "source_text"));
        }
        let source_text = String::from_utf8(bytes[idx..idx + src_len].to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "source_text"))?;
        Ok(Self {
            id: L1Id(id),
            role,
            polarity,
            abstraction,
            vector: normalize_with_dim(&vector, D_SEM),
            source_text,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConceptId(pub u64);

impl Codec for ConceptId {
    fn encode(&self) -> Vec<u8> {
        self.0.to_le_bytes().to_vec()
    }

    fn decode(bytes: &[u8]) -> io::Result<Self> {
        if bytes.len() != 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid ConceptId",
            ));
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(bytes);
        Ok(Self(u64::from_le_bytes(buf)))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConceptUnit {
    pub id: ConceptId,
    pub l1_refs: Vec<L1Id>,
    pub v: Vec<f32>,
    pub a: f32,
    pub s: Vec<f32>,
    pub polarity: i8,
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
        out.push(self.polarity as u8);
        out.extend_from_slice(&self.timestamp.to_le_bytes());
        out.extend_from_slice(&(self.l1_refs.len() as u32).to_le_bytes());
        for id in &self.l1_refs {
            out.extend_from_slice(&id.0.to_le_bytes());
        }
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

        let (polarity, timestamp) = if idx.saturating_add(8) == bytes.len() {
            (0, read_u64(bytes, &mut idx)?)
        } else {
            let p = read_u8(bytes, &mut idx)? as i8;
            (normalize_polarity_i8(p), read_u64(bytes, &mut idx)?)
        };

        let l1_refs = if idx < bytes.len() {
            let refs_len = read_u32(bytes, &mut idx)? as usize;
            let mut refs = Vec::with_capacity(refs_len);
            for _ in 0..refs_len {
                refs.push(L1Id(read_u128(bytes, &mut idx)?));
            }
            refs
        } else {
            Vec::new()
        };

        Ok(Self {
            id: ConceptId(id),
            l1_refs,
            v,
            a,
            s,
            polarity,
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
    pub polarity: i8,
}

impl ConceptQuery {
    pub fn normalized(self) -> Self {
        Self {
            v: normalize_with_dim(&self.v, D_SEM),
            a: self.a.clamp(0.0, 1.0),
            s: normalize_with_dim(&self.s, D_STRUCT),
            polarity: normalize_polarity_i8(self.polarity),
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

pub struct SemanticL1Dhm<S>
where
    S: Store<L1Id, SemanticUnitL1>,
{
    store: S,
    next_id: u128,
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
            l1_refs: Vec::new(),
            v: q.v,
            a: q.a,
            s: q.s,
            polarity: q.polarity,
            timestamp: now_ts(),
        };

        self.store.put(id, unit).expect("failed to store concept");
        id
    }

    pub fn get(&self, id: ConceptId) -> Option<ConceptUnit> {
        self.store.get(&id).unwrap_or(None)
    }

    pub fn all_concepts(&self) -> Vec<ConceptUnit> {
        let mut entries = self.store.entries().unwrap_or_default();
        entries.sort_by(|(lid, _), (rid, _)| lid.cmp(rid));
        entries.into_iter().map(|(_, concept)| concept).collect()
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

    pub fn insert_from_l1_units(&mut self, l1_units: &[SemanticUnitL1]) -> ConceptId {
        let query = query_from_l1_units(l1_units);
        let id = ConceptId(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let unit = ConceptUnit {
            id,
            l1_refs: l1_units.iter().map(|u| u.id).collect(),
            v: query.v,
            a: query.a,
            s: query.s,
            polarity: query.polarity,
            timestamp: now_ts(),
        };
        self.store.put(id, unit).expect("failed to store concept");
        id
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

impl<S> SemanticL1Dhm<S>
where
    S: Store<L1Id, SemanticUnitL1>,
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

    pub fn insert(&mut self, input: &SemanticUnitL1Input) -> L1Id {
        let id = L1Id(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let unit = SemanticUnitL1 {
            id,
            role: input.role,
            polarity: normalize_polarity_i8(input.polarity),
            abstraction: input.abstraction.clamp(0.0, 1.0),
            vector: normalize_with_dim(&input.vector, D_SEM),
            source_text: input.source_text.clone(),
        };
        self.store.put(id, unit).expect("failed to store l1 unit");
        id
    }

    pub fn get(&self, id: L1Id) -> Option<SemanticUnitL1> {
        self.store.get(&id).unwrap_or(None)
    }

    pub fn all_units(&self) -> Vec<SemanticUnitL1> {
        let mut entries = self.store.entries().unwrap_or_default();
        entries.sort_by(|(l, _), (r, _)| l.cmp(r));
        entries.into_iter().map(|(_, unit)| unit).collect()
    }
}

impl SemanticL1Dhm<InMemoryStore<L1Id, SemanticUnitL1>> {
    pub fn in_memory() -> io::Result<Self> {
        Self::new(InMemoryStore::new())
    }
}

impl SemanticL1Dhm<FileStore<L1Id, SemanticUnitL1>> {
    pub fn file(path: impl AsRef<Path>) -> io::Result<Self> {
        Self::new(FileStore::open(path)?)
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
        polarity: normalize_polarity_i8(m.polarity),
    }
}

pub fn query_from_l1_units(units: &[SemanticUnitL1]) -> ConceptQuery {
    if units.is_empty() {
        return ConceptQuery {
            v: vec![0.0; D_SEM],
            a: 0.0,
            s: vec![0.0; D_STRUCT],
            polarity: 0,
        };
    }

    let mut v_acc = vec![0.0f32; D_SEM];
    let mut s_acc = vec![0.0f32; D_STRUCT];
    let mut a_sum = 0.0f32;
    let mut p_sum = 0i32;

    for unit in units {
        let v = normalize_with_dim(&unit.vector, D_SEM);
        add_scaled(&mut v_acc, &v, 1.0);
        add_scaled(&mut s_acc, &v, role_weight_from_requirement(unit.role));
        a_sum += unit.abstraction.clamp(0.0, 1.0);
        p_sum += unit.polarity as i32;
    }

    let polarity = if p_sum > 0 {
        1
    } else if p_sum < 0 {
        -1
    } else {
        0
    };

    ConceptQuery {
        v: normalize_with_dim(&v_acc, D_SEM),
        a: (a_sum / units.len() as f32).clamp(0.0, 1.0),
        s: normalize_with_dim(&s_acc, D_STRUCT),
        polarity,
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
    let polarity = if a.polarity == b.polarity {
        a.polarity
    } else {
        0
    };
    ConceptQuery {
        v,
        a: abs,
        s,
        polarity,
    }
}

pub fn abstract_move(c: &ConceptUnit, delta: f32) -> ConceptQuery {
    ConceptQuery {
        v: normalize_with_dim(&c.v, D_SEM),
        a: (c.a + delta).clamp(0.0, 1.0),
        s: normalize_with_dim(&c.s, D_STRUCT),
        polarity: normalize_polarity_i8(c.polarity),
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
        polarity: normalize_polarity_i8(c.polarity),
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

fn read_u128(raw: &[u8], idx: &mut usize) -> io::Result<u128> {
    if idx.saturating_add(16) > raw.len() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "u128"));
    }
    let mut buf = [0u8; 16];
    buf.copy_from_slice(&raw[*idx..*idx + 16]);
    *idx += 16;
    Ok(u128::from_le_bytes(buf))
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

fn read_u8(raw: &[u8], idx: &mut usize) -> io::Result<u8> {
    if idx.saturating_add(1) > raw.len() {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "u8"));
    }
    let value = raw[*idx];
    *idx += 1;
    Ok(value)
}

fn normalize_polarity_i8(p: i8) -> i8 {
    match p.cmp(&0) {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Greater => 1,
        std::cmp::Ordering::Equal => 0,
    }
}

fn role_to_u8(role: RequirementRole) -> u8 {
    match role {
        RequirementRole::Goal => 0,
        RequirementRole::Constraint => 1,
        RequirementRole::Optimization => 2,
        RequirementRole::Prohibition => 3,
    }
}

fn role_from_u8(raw: u8) -> io::Result<RequirementRole> {
    match raw {
        0 => Ok(RequirementRole::Goal),
        1 => Ok(RequirementRole::Constraint),
        2 => Ok(RequirementRole::Optimization),
        3 => Ok(RequirementRole::Prohibition),
        _ => Err(io::Error::new(io::ErrorKind::InvalidData, "invalid role")),
    }
}

fn role_weight_from_requirement(role: RequirementRole) -> f32 {
    match role {
        RequirementRole::Goal => 1.0,
        RequirementRole::Constraint => 1.2,
        RequirementRole::Optimization => 0.9,
        RequirementRole::Prohibition => 1.3,
    }
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
            polarity: 0,
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

    #[test]
    fn l1_store_roundtrip_and_l2_refs() {
        let mut l1 = SemanticL1Dhm::in_memory().expect("l1");
        let l1_id = l1.insert(&SemanticUnitL1Input {
            role: RequirementRole::Goal,
            polarity: 1,
            abstraction: 0.8,
            vector: vec![1.0; D_SEM],
            source_text: "高速化したい".to_string(),
        });
        let unit = l1.get(l1_id).expect("unit");
        assert_eq!(unit.role, RequirementRole::Goal);
        assert_eq!(unit.polarity, 1);
        assert_eq!(unit.source_text, "高速化したい");

        let mut dhm = SemanticDhm::in_memory().expect("dhm");
        let c_id = dhm.insert_from_l1_units(&[unit]);
        let concept = dhm.get(c_id).expect("concept");
        assert_eq!(concept.l1_refs.len(), 1);
        assert_eq!(concept.l1_refs[0], l1_id);
    }
}
