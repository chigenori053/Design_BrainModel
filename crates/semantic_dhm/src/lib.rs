use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use meaning_extractor::{MeaningStructure, NodeId, RelationType, RoleType};
use memory_store::{Codec, FileStore, InMemoryStore, Store};

#[derive(Debug)]
pub enum SemanticError {
    InvalidInput(String),
    MissingField(&'static str),
    InconsistentState(&'static str),
    EvaluationError(String),
    SnapshotError(String),
}

impl std::fmt::Display for SemanticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(msg) => write!(f, "{msg}"),
            Self::MissingField(name) => write!(f, "missing field: {name}"),
            Self::InconsistentState(msg) => write!(f, "inconsistent state: {msg}"),
            Self::EvaluationError(msg) => write!(f, "evaluation error: {msg}"),
            Self::SnapshotError(msg) => write!(f, "snapshot error: {msg}"),
        }
    }
}

impl std::error::Error for SemanticError {}

impl From<io::Error> for SemanticError {
    fn from(value: io::Error) -> Self {
        SemanticError::EvaluationError(value.to_string())
    }
}

pub const D_SEM: usize = 384;
pub const D_STRUCT: usize = 384;
pub const SIM_PRECISION: f64 = 1000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct L2Config {
    pub similarity_threshold: f64,
    pub algorithm_version: u32,
}

pub const DEFAULT_L2_CONFIG: L2Config = L2Config {
    similarity_threshold: 0.995,
    algorithm_version: 1,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum L2Mode {
    Stable,
    Experimental(L2Config),
}

#[derive(Clone, Debug, PartialEq)]
pub struct MeaningLayerSnapshot {
    pub algorithm_version: u32,
    pub l1: Vec<L1Snapshot>,
    pub l2: Vec<L2Snapshot>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct L1Snapshot {
    pub id: L1Id,
    pub role: RequirementRole,
    pub polarity: i8,
    pub abstraction: f32,
    pub vector_hash: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct L2Snapshot {
    pub id: L2Id,
    pub l1_refs: Vec<L1Id>,
    pub integrated_vector_hash: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SnapshotDiff {
    pub identical: bool,
    pub algorithm_version_changed: bool,
    pub l1_changed: bool,
    pub l2_changed: bool,
}

pub trait Snapshotable {
    fn snapshot(&self) -> MeaningLayerSnapshot;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RequirementKind {
    Performance,
    Memory,
    Security,
    NoCloud,
    Reliability,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DerivedRequirement {
    pub kind: RequirementKind,
    pub strength: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesignProjection {
    pub source_l2_ids: Vec<L2Id>,
    pub derived: Vec<DerivedRequirement>,
}

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
pub type L2Id = ConceptId;

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
    pub integrated_vector: Vec<f32>,
    pub a: f32,
    pub s: Vec<f32>,
    pub polarity: i8,
    pub timestamp: u64,
}

impl Codec for ConceptUnit {
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&self.id.0.to_le_bytes());
        out.extend_from_slice(&(self.integrated_vector.len() as u32).to_le_bytes());
        for x in &self.integrated_vector {
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
            integrated_vector: v,
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
    l2_config: L2Config,
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
            l2_config: DEFAULT_L2_CONFIG,
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
            integrated_vector: q.v,
            a: q.a,
            s: q.s,
            polarity: q.polarity,
            timestamp: now_ts(),
        };

        let _ = self.store.put(id, unit);
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

    pub fn l2_config(&self) -> L2Config {
        self.l2_config
    }

    pub fn insert_from_l1_units(&mut self, l1_units: &[SemanticUnitL1]) -> ConceptId {
        let unit = build_l2_unit_from_l1(l1_units, self.l2_config);
        let id = unit.id;
        let _ = self.store.put(id, unit);
        self.next_id = self.next_id.max(id.0.saturating_add(1));
        id
    }

    pub fn rebuild_l2_from_l1(&mut self, l1_units: &[SemanticUnitL1]) -> Result<(), SemanticError> {
        self.rebuild_l2_from_l1_with_config(l1_units, DEFAULT_L2_CONFIG)
    }

    pub fn rebuild_l2_from_l1_with_config(
        &mut self,
        l1_units: &[SemanticUnitL1],
        config: L2Config,
    ) -> Result<(), SemanticError> {
        let rebuilt = build_l2_cache_with_config(l1_units, config);
        let entries = rebuilt
            .into_iter()
            .map(|unit| (unit.id, unit))
            .collect::<Vec<_>>();
        self.store
            .replace_all(entries)
            .map_err(|e| SemanticError::EvaluationError(e.to_string()))?;
        self.next_id = self
            .store
            .entries()
            .map_err(|e| SemanticError::EvaluationError(e.to_string()))?
            .into_iter()
            .map(|(id, _)| id.0)
            .max()
            .map(|v| v.saturating_add(1))
            .unwrap_or(1);
        self.l2_config = config;
        Ok(())
    }

    pub fn rebuild_l2_from_l1_with_mode(
        &mut self,
        l1_units: &[SemanticUnitL1],
        mode: L2Mode,
    ) -> Result<(), SemanticError> {
        match mode {
            L2Mode::Stable => self.rebuild_l2_from_l1_with_config(l1_units, DEFAULT_L2_CONFIG),
            L2Mode::Experimental(config) => self.rebuild_l2_from_l1_with_config(l1_units, config),
        }
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
        let _ = self.store.put(id, unit);
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

    pub fn remove(&mut self, id: L1Id) -> io::Result<()> {
        let kept = self
            .store
            .entries()?
            .into_iter()
            .filter(|(candidate, _)| *candidate != id)
            .collect::<Vec<_>>();
        self.store.replace_all(kept)?;
        self.next_id = self
            .store
            .entries()?
            .into_iter()
            .map(|(key, _)| key.0)
            .max()
            .map(|v| v.saturating_add(1))
            .unwrap_or(1);
        Ok(())
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

pub fn normalized_l1(mut l1_units: Vec<SemanticUnitL1>) -> Vec<SemanticUnitL1> {
    l1_units.sort_by(|l, r| l.id.cmp(&r.id));
    l1_units
}

pub fn deterministic_grouping(l1_units: &[SemanticUnitL1]) -> Vec<Vec<L1Id>> {
    deterministic_grouping_with_config(l1_units, DEFAULT_L2_CONFIG)
}

pub fn deterministic_grouping_with_config(
    l1_units: &[SemanticUnitL1],
    config: L2Config,
) -> Vec<Vec<L1Id>> {
    if l1_units.is_empty() {
        return Vec::new();
    }

    let normalized = normalized_l1(l1_units.to_vec());
    let n = normalized.len();
    let mut uf = UnionFind::new(n);

    for i in 0..n {
        for j in (i + 1)..n {
            let sim = cosine_similarity(&normalized[i].vector, &normalized[j].vector);
            let qsim = quantize_similarity(sim);
            let qth = quantize_similarity(config.similarity_threshold);
            if qsim >= qth {
                uf.union(i, j);
            }
        }
    }

    let mut groups = BTreeMap::<usize, Vec<L1Id>>::new();
    for (idx, unit) in normalized.iter().enumerate() {
        let root = uf.find(idx);
        groups.entry(root).or_default().push(unit.id);
    }

    let mut out = groups.into_values().collect::<Vec<_>>();
    for group in &mut out {
        group.sort();
    }
    out.sort();
    out
}

pub fn build_l2_cache(l1_units: &[SemanticUnitL1]) -> Vec<ConceptUnit> {
    build_l2_cache_with_config(l1_units, DEFAULT_L2_CONFIG)
}

pub fn build_l2_cache_with_config(
    l1_units: &[SemanticUnitL1],
    config: L2Config,
) -> Vec<ConceptUnit> {
    let normalized = normalized_l1(l1_units.to_vec());
    let by_id = normalized
        .iter()
        .map(|u| (u.id, u.clone()))
        .collect::<BTreeMap<_, _>>();
    let groups = deterministic_grouping_with_config(&normalized, config);
    let mut out = Vec::with_capacity(groups.len());
    for refs in groups {
        let members = refs
            .iter()
            .filter_map(|id| by_id.get(id).cloned())
            .collect::<Vec<_>>();
        out.push(build_l2_unit_from_l1(&members, config));
    }
    out.sort_by(|l, r| l.id.cmp(&r.id));
    out
}

#[derive(Clone, Debug)]
pub struct MeaningLayerState {
    pub algorithm_version: u32,
    pub l1_units: Vec<SemanticUnitL1>,
    pub l2_units: Vec<ConceptUnit>,
}

impl Snapshotable for MeaningLayerState {
    fn snapshot(&self) -> MeaningLayerSnapshot {
        let mut l1 = self
            .l1_units
            .iter()
            .map(|u| L1Snapshot {
                id: u.id,
                role: u.role,
                polarity: u.polarity,
                abstraction: quantize_f32(u.abstraction, SIM_PRECISION as f32),
                vector_hash: hash_quantized_vector(&u.vector),
            })
            .collect::<Vec<_>>();
        l1.sort_by(|l, r| l.id.cmp(&r.id));

        let mut l2 = self
            .l2_units
            .iter()
            .map(|u| {
                let mut refs = u.l1_refs.clone();
                refs.sort();
                L2Snapshot {
                    id: u.id,
                    l1_refs: refs,
                    integrated_vector_hash: hash_quantized_vector(&u.integrated_vector),
                }
            })
            .collect::<Vec<_>>();
        l2.sort_by(|l, r| l.id.cmp(&r.id));

        MeaningLayerSnapshot {
            algorithm_version: self.algorithm_version,
            l1,
            l2,
        }
    }
}

pub fn compare_snapshots(
    a: &MeaningLayerSnapshot,
    b: &MeaningLayerSnapshot,
) -> Result<SnapshotDiff, SemanticError> {
    if a.algorithm_version == b.algorithm_version && (a.l1.is_empty() != b.l1.is_empty()) {
        return Err(SemanticError::SnapshotError(
            "l1 snapshot cardinality mismatch".to_string(),
        ));
    }
    Ok(SnapshotDiff {
        identical: a == b,
        algorithm_version_changed: a.algorithm_version != b.algorithm_version,
        l1_changed: a.l1 != b.l1,
        l2_changed: a.l2 != b.l2,
    })
}

pub fn project_phase_a(l2_units: &[ConceptUnit], l1_units: &[SemanticUnitL1]) -> DesignProjection {
    let mut sorted_l2 = l2_units.to_vec();
    sorted_l2.sort_by(|l, r| l.id.cmp(&r.id));
    let source_l2_ids = sorted_l2.iter().map(|u| u.id).collect::<Vec<_>>();

    let l1_by_id = l1_units
        .iter()
        .map(|u| (u.id, u.clone()))
        .collect::<BTreeMap<_, _>>();

    let mut sums = BTreeMap::<RequirementKind, f32>::new();
    for l2 in &sorted_l2 {
        let mut refs = l2.l1_refs.clone();
        refs.sort();
        for id in refs {
            let Some(l1) = l1_by_id.get(&id) else {
                continue;
            };
            let kind = infer_requirement_kind(l1);
            let strength = role_projection_weight(l1.role)
                * l1.abstraction.clamp(0.0, 1.0)
                * (l1.polarity as f32);
            *sums.entry(kind).or_insert(0.0) += strength;
        }
    }

    let mut derived = sums
        .into_iter()
        .map(|(kind, strength)| DerivedRequirement {
            kind,
            strength: quantize_f32(strength, SIM_PRECISION as f32),
        })
        .collect::<Vec<_>>();
    derived.sort_by(|l, r| l.kind.cmp(&r.kind));

    DesignProjection {
        source_l2_ids,
        derived,
    }
}

pub fn generate_l2_id(l1_refs: &[L1Id], algorithm_version: u32) -> ConceptId {
    let mut sorted = l1_refs.to_vec();
    sorted.sort();
    if sorted.len() == 1 && algorithm_version == DEFAULT_L2_CONFIG.algorithm_version {
        let only = sorted.first().copied().unwrap_or(L1Id(1));
        return ConceptId((only.0 as u64).max(1));
    }
    let mut hash: u64 = 1469598103934665603;
    for b in algorithm_version.to_le_bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    for id in sorted {
        for b in id.0.to_le_bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(1099511628211);
        }
    }
    ConceptId(hash.max(1))
}

fn build_l2_unit_from_l1(l1_units: &[SemanticUnitL1], config: L2Config) -> ConceptUnit {
    let mut refs = l1_units.iter().map(|u| u.id).collect::<Vec<_>>();
    refs.sort();
    let query = query_from_l1_units(l1_units);
    ConceptUnit {
        id: generate_l2_id(&refs, config.algorithm_version),
        l1_refs: refs,
        integrated_vector: query.v,
        a: query.a,
        s: query.s,
        polarity: query.polarity,
        timestamp: 0,
    }
}

pub fn resonance(query: &ConceptQuery, c: &ConceptUnit, weights: ResonanceWeights) -> f32 {
    let w = weights.normalized();
    let q = query.clone().normalized();
    let cv = normalize_with_dim(&c.integrated_vector, D_SEM);
    let cs = normalize_with_dim(&c.s, D_STRUCT);

    w.gamma1 * dot(&q.v, &cv) + w.gamma2 * dot(&q.s, &cs) - w.gamma3 * (q.a - c.a).abs()
}

pub fn energy(query: &ConceptQuery, c: &ConceptUnit, weights: ResonanceWeights) -> f32 {
    -resonance(query, c, weights)
}

pub fn fuse(a: &ConceptUnit, b: &ConceptUnit) -> ConceptQuery {
    let v = normalize_with_dim(&add_vec(&a.integrated_vector, &b.integrated_vector), D_SEM);
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
        v: normalize_with_dim(&c.integrated_vector, D_SEM),
        a: (c.a + delta).clamp(0.0, 1.0),
        s: normalize_with_dim(&c.s, D_STRUCT),
        polarity: normalize_polarity_i8(c.polarity),
    }
}

pub fn repulse(c: &ConceptUnit, conflict: &[f32], lambda: f32) -> ConceptQuery {
    let conf = normalize_with_dim(conflict, D_SEM);
    let mut shifted = normalize_with_dim(&c.integrated_vector, D_SEM);
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

fn cosine_similarity(a: &[f32], b: &[f32]) -> f64 {
    let an = normalize_with_dim(a, D_SEM);
    let bn = normalize_with_dim(b, D_SEM);
    let sim = dot(&an, &bn) as f64;
    sim.clamp(-1.0, 1.0)
}

fn quantize_similarity(similarity: f64) -> i64 {
    (similarity.clamp(-1.0, 1.0) * SIM_PRECISION).round() as i64
}

#[derive(Debug)]
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }

    fn find(&mut self, x: usize) -> usize {
        if self.parent[x] != x {
            let root = self.find(self.parent[x]);
            self.parent[x] = root;
        }
        self.parent[x]
    }

    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb {
            return;
        }
        if ra < rb {
            self.parent[rb] = ra;
        } else {
            self.parent[ra] = rb;
        }
    }
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

fn role_projection_weight(role: RequirementRole) -> f32 {
    match role {
        RequirementRole::Goal => 1.0,
        RequirementRole::Optimization => 0.7,
        RequirementRole::Constraint => 1.2,
        RequirementRole::Prohibition => -1.2,
    }
}

fn infer_requirement_kind(l1: &SemanticUnitL1) -> RequirementKind {
    let text = l1.source_text.to_ascii_lowercase();
    if text.contains("cloud") || l1.source_text.contains("クラウド") {
        RequirementKind::NoCloud
    } else if text.contains("security") || l1.source_text.contains("セキュリ") {
        RequirementKind::Security
    } else if text.contains("memory") || l1.source_text.contains("メモリ") {
        RequirementKind::Memory
    } else if text.contains("reliab") || l1.source_text.contains("信頼") {
        RequirementKind::Reliability
    } else {
        RequirementKind::Performance
    }
}

fn quantize_f32(value: f32, precision: f32) -> f32 {
    (value * precision).round() / precision
}

fn hash_quantized_vector(vector: &[f32]) -> u64 {
    let mut hasher = Fnv1a64::new();
    for v in vector {
        let q = (f64::from(*v).clamp(-1.0, 1.0) * SIM_PRECISION).round() as i64;
        hasher.write_i64(q);
    }
    hasher.finish()
}

#[derive(Debug, Clone, Copy)]
struct Fnv1a64 {
    state: u64,
}

impl Fnv1a64 {
    fn new() -> Self {
        Self {
            state: 0xcbf29ce484222325,
        }
    }

    fn write_i64(&mut self, value: i64) {
        for b in value.to_le_bytes() {
            self.state ^= u64::from(b);
            self.state = self.state.wrapping_mul(0x100000001b3);
        }
    }

    fn finish(self) -> u64 {
        self.state
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

    #[test]
    fn l2_order_invariance() {
        let units = vec![
            SemanticUnitL1 {
                id: L1Id(10),
                role: RequirementRole::Goal,
                polarity: 1,
                abstraction: 0.7,
                vector: vec![1.0; D_SEM],
                source_text: "goal".to_string(),
            },
            SemanticUnitL1 {
                id: L1Id(20),
                role: RequirementRole::Constraint,
                polarity: -1,
                abstraction: 0.2,
                vector: vec![0.2; D_SEM],
                source_text: "constraint".to_string(),
            },
            SemanticUnitL1 {
                id: L1Id(30),
                role: RequirementRole::Optimization,
                polarity: 1,
                abstraction: 0.5,
                vector: vec![0.95; D_SEM],
                source_text: "optimization".to_string(),
            },
        ];

        let a = build_l2_cache(&units);
        let mut b_input = units.clone();
        b_input.swap(0, 2);
        let b = build_l2_cache(&b_input);

        assert_eq!(a, b);
    }

    #[test]
    fn rebuild_from_l1_matches_direct_build() {
        let mut l1 = SemanticL1Dhm::in_memory().expect("l1");
        let mut dhm = SemanticDhm::in_memory().expect("dhm");

        let u1 = l1.insert(&SemanticUnitL1Input {
            role: RequirementRole::Goal,
            polarity: 1,
            abstraction: 0.8,
            vector: vec![1.0; D_SEM],
            source_text: "高速化".to_string(),
        });
        let u2 = l1.insert(&SemanticUnitL1Input {
            role: RequirementRole::Prohibition,
            polarity: -1,
            abstraction: 0.4,
            vector: vec![-1.0; D_SEM],
            source_text: "禁止".to_string(),
        });

        let l1_units = vec![l1.get(u1).expect("u1"), l1.get(u2).expect("u2")];
        let expected = build_l2_cache(&l1_units);

        dhm.rebuild_l2_from_l1(&l1.all_units()).expect("rebuild");
        let rebuilt = dhm.all_concepts();
        assert_eq!(expected, rebuilt);
    }

    #[test]
    fn removing_l1_and_rebuild_removes_references() {
        let mut l1 = SemanticL1Dhm::in_memory().expect("l1");
        let mut dhm = SemanticDhm::in_memory().expect("dhm");

        let kept = l1.insert(&SemanticUnitL1Input {
            role: RequirementRole::Goal,
            polarity: 1,
            abstraction: 0.8,
            vector: vec![1.0; D_SEM],
            source_text: "keep".to_string(),
        });
        let removed = l1.insert(&SemanticUnitL1Input {
            role: RequirementRole::Constraint,
            polarity: -1,
            abstraction: 0.2,
            vector: vec![-1.0; D_SEM],
            source_text: "remove".to_string(),
        });

        dhm.rebuild_l2_from_l1(&l1.all_units()).expect("rebuild");
        l1.remove(removed).expect("remove");
        dhm.rebuild_l2_from_l1(&l1.all_units())
            .expect("rebuild after remove");

        for concept in dhm.all_concepts() {
            assert!(!concept.l1_refs.contains(&removed));
        }
        assert!(
            dhm.all_concepts()
                .iter()
                .all(|c| c.l1_refs.iter().all(|id| *id == kept))
        );
    }

    #[test]
    fn l2_id_depends_on_algorithm_version() {
        let refs = vec![L1Id(1), L1Id(2), L1Id(3)];
        let v1 = generate_l2_id(&refs, 1);
        let v2 = generate_l2_id(&refs, 2);
        assert_ne!(v1, v2);
    }

    #[test]
    fn stable_and_experimental_modes_differ_by_version() {
        let units = vec![
            SemanticUnitL1 {
                id: L1Id(100),
                role: RequirementRole::Goal,
                polarity: 1,
                abstraction: 0.6,
                vector: vec![1.0; D_SEM],
                source_text: "a".to_string(),
            },
            SemanticUnitL1 {
                id: L1Id(200),
                role: RequirementRole::Goal,
                polarity: 1,
                abstraction: 0.6,
                vector: vec![0.99; D_SEM],
                source_text: "b".to_string(),
            },
        ];
        let stable = build_l2_cache_with_config(&units, DEFAULT_L2_CONFIG);
        let experimental = build_l2_cache_with_config(
            &units,
            L2Config {
                similarity_threshold: DEFAULT_L2_CONFIG.similarity_threshold,
                algorithm_version: DEFAULT_L2_CONFIG.algorithm_version + 1,
            },
        );
        assert_ne!(stable, experimental);
    }

    #[test]
    fn snapshot_is_deterministic_and_order_invariant() {
        let l1_a = SemanticUnitL1 {
            id: L1Id(1),
            role: RequirementRole::Goal,
            polarity: 1,
            abstraction: 0.7,
            vector: vec![1.0; D_SEM],
            source_text: "performance".to_string(),
        };
        let l1_b = SemanticUnitL1 {
            id: L1Id(2),
            role: RequirementRole::Prohibition,
            polarity: -1,
            abstraction: 0.6,
            vector: vec![0.5; D_SEM],
            source_text: "no cloud".to_string(),
        };

        let l2 = build_l2_cache(&[l1_a.clone(), l1_b.clone()]);
        let s1 = MeaningLayerState {
            algorithm_version: DEFAULT_L2_CONFIG.algorithm_version,
            l1_units: vec![l1_a.clone(), l1_b.clone()],
            l2_units: l2.clone(),
        }
        .snapshot();
        let s2 = MeaningLayerState {
            algorithm_version: DEFAULT_L2_CONFIG.algorithm_version,
            l1_units: vec![l1_b, l1_a],
            l2_units: l2,
        }
        .snapshot();
        assert_eq!(s1, s2);
        let diff = compare_snapshots(&s1, &s2).expect("snapshot compare should succeed");
        assert!(diff.identical);
    }

    #[test]
    fn projection_phase_a_is_deterministic() {
        let l1 = vec![
            SemanticUnitL1 {
                id: L1Id(11),
                role: RequirementRole::Goal,
                polarity: 1,
                abstraction: 0.9,
                vector: vec![1.0; D_SEM],
                source_text: "security hardening".to_string(),
            },
            SemanticUnitL1 {
                id: L1Id(12),
                role: RequirementRole::Prohibition,
                polarity: -1,
                abstraction: 0.8,
                vector: vec![0.8; D_SEM],
                source_text: "no cloud dependency".to_string(),
            },
        ];
        let l2 = build_l2_cache(&l1);
        let p1 = project_phase_a(&l2, &l1);
        let mut l2_rev = l2.clone();
        l2_rev.reverse();
        let p2 = project_phase_a(&l2_rev, &l1);
        assert_eq!(p1, p2);
    }
}
