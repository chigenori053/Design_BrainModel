use std::collections::BTreeMap;

use memory_space::{DesignNode, DesignState, Value};
use num_complex::Complex;

pub type Scalar = Complex<f32>;

#[derive(Clone, Debug, PartialEq)]
pub struct FieldVector {
    pub data: Vec<Scalar>,
}

impl FieldVector {
    pub fn zeros(dimensions: usize) -> Self {
        Self {
            data: vec![Complex::new(0.0, 0.0); dimensions],
        }
    }

    pub fn dimensions(&self) -> usize {
        self.data.len()
    }

    pub fn scale(&self, factor: f32) -> Self {
        let data = self.data.iter().map(|v| *v * factor).collect();
        Self { data }
    }

    pub fn add(&self, other: &Self) -> Self {
        let len = self.dimensions().min(other.dimensions());
        let mut data = Vec::with_capacity(len);
        for i in 0..len {
            data.push(self.data[i] + other.data[i]);
        }
        Self { data }
    }

    pub fn sub(&self, other: &Self) -> Self {
        let len = self.dimensions().min(other.dimensions());
        let mut data = Vec::with_capacity(len);
        for i in 0..len {
            data.push(self.data[i] - other.data[i]);
        }
        Self { data }
    }

    pub fn normalized(&self) -> Self {
        let norm_sq: f32 = self.data.iter().map(|v| v.norm_sqr()).sum();
        if norm_sq <= f32::EPSILON {
            return self.clone();
        }
        let norm = norm_sq.sqrt();
        let data = self.data.iter().map(|v| *v / norm).collect();
        Self { data }
    }

    pub fn average(vectors: &[FieldVector], dimensions: usize) -> Self {
        if vectors.is_empty() {
            return FieldVector::zeros(dimensions);
        }

        let mut acc = FieldVector::zeros(dimensions);
        for v in vectors {
            acc = acc.add(v);
        }
        acc.scale(1.0 / vectors.len() as f32)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NodeCategory {
    Interface,
    Storage,
    Network,
    Compute,
    Control,
    Constraint,
    Abstraction,
    Performance,
    Reliability,
    CostSensitive,
}

impl NodeCategory {
    fn all() -> [NodeCategory; 10] {
        [
            NodeCategory::Interface,
            NodeCategory::Storage,
            NodeCategory::Network,
            NodeCategory::Compute,
            NodeCategory::Control,
            NodeCategory::Constraint,
            NodeCategory::Abstraction,
            NodeCategory::Performance,
            NodeCategory::Reliability,
            NodeCategory::CostSensitive,
        ]
    }

    fn index(self) -> usize {
        match self {
            NodeCategory::Interface => 0,
            NodeCategory::Storage => 1,
            NodeCategory::Network => 2,
            NodeCategory::Compute => 3,
            NodeCategory::Control => 4,
            NodeCategory::Constraint => 5,
            NodeCategory::Abstraction => 6,
            NodeCategory::Performance => 7,
            NodeCategory::Reliability => 8,
            NodeCategory::CostSensitive => 9,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TargetField {
    pub data: FieldVector,
}

impl TargetField {
    pub fn fixed(dimensions: usize) -> Self {
        let projector = HybridProjector::new(dimensions, 0.8, 0.2);
        let pseudo_node = DesignNode {
            id: memory_space::Uuid::from_u128(0xFFFF),
            kind: "Target".to_string(),
            attributes: BTreeMap::new(),
        };
        let data = projector.project_for_category(&pseudo_node, NodeCategory::Abstraction);
        Self { data }
    }

    pub fn blend(global: &FieldVector, local: &FieldVector, lambda: f32) -> Self {
        let l = lambda.clamp(0.0, 1.0);
        let data = global.scale(l).add(&local.scale(1.0 - l));
        Self { data }
    }
}

pub trait NodeProjector {
    fn project(&self, node: &DesignNode) -> FieldVector;
}

#[derive(Clone, Debug, PartialEq)]
pub struct HybridProjector {
    dimension: usize,
    alpha: f32,
    beta: f32,
    category_basis: BTreeMap<NodeCategory, FieldVector>,
}

impl HybridProjector {
    pub fn new(dimension: usize, alpha: f32, beta: f32) -> Self {
        assert!(dimension > 0);
        assert!(dimension <= 1024);

        let mut category_basis = BTreeMap::new();
        for category in NodeCategory::all() {
            category_basis.insert(
                category,
                build_category_basis(dimension, category.index() as u64),
            );
        }

        Self {
            dimension,
            alpha,
            beta,
            category_basis,
        }
    }

    pub fn default_coefficients(dimension: usize) -> Self {
        Self::new(dimension, 0.8, 0.2)
    }

    pub fn dimensions(&self) -> usize {
        self.dimension
    }

    pub fn alpha(&self) -> f32 {
        self.alpha
    }

    pub fn beta(&self) -> f32 {
        self.beta
    }

    pub fn basis_for(&self, category: NodeCategory) -> FieldVector {
        self.category_basis
            .get(&category)
            .cloned()
            .unwrap_or_else(|| FieldVector::zeros(self.dimension))
    }

    fn hash_projection(&self, node: &DesignNode) -> FieldVector {
        let seed = stable_hash_node(node);
        let mut data = Vec::with_capacity(self.dimension);

        for i in 0..self.dimension {
            let x = splitmix64(seed.wrapping_add((i as u64).wrapping_mul(0x9e3779b97f4a7c15)));
            let re = if (x & 0b01) == 0 { 1.0 } else { -1.0 };
            let im = if (x & 0b10) == 0 { 1.0 } else { -1.0 };
            data.push(Complex::new(re, im));
        }

        FieldVector { data }
    }

    fn category_projection(&self, node: &DesignNode) -> FieldVector {
        let category = infer_category(node);
        self.category_basis
            .get(&category)
            .cloned()
            .unwrap_or_else(|| FieldVector::zeros(self.dimension))
    }

    fn project_for_category(&self, node: &DesignNode, category: NodeCategory) -> FieldVector {
        let h = self.hash_projection(node);
        let c = self
            .category_basis
            .get(&category)
            .cloned()
            .unwrap_or_else(|| FieldVector::zeros(self.dimension));

        h.scale(self.alpha).add(&c.scale(self.beta))
    }
}

impl NodeProjector for HybridProjector {
    fn project(&self, node: &DesignNode) -> FieldVector {
        let h = self.hash_projection(node);
        let c = self.category_projection(node);
        h.scale(self.alpha).add(&c.scale(self.beta))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct FieldEngine {
    dimensions: usize,
    projector: HybridProjector,
}

impl FieldEngine {
    pub fn new(dimensions: usize) -> Self {
        let projector = HybridProjector::default_coefficients(dimensions);
        Self {
            dimensions,
            projector,
        }
    }

    pub fn dimensions(&self) -> usize {
        self.dimensions
    }

    pub fn projector(&self) -> &HybridProjector {
        &self.projector
    }

    pub fn project_node(&self, node: &DesignNode) -> FieldVector {
        self.projector.project(node)
    }

    pub fn aggregate_state(&self, state: &DesignState) -> FieldVector {
        let nodes: Vec<DesignNode> = state.graph.nodes().values().cloned().collect();
        self.aggregate_nodes(&nodes)
    }

    pub fn aggregate_nodes(&self, nodes: &[DesignNode]) -> FieldVector {
        aggregate_with_projector(nodes, &self.projector)
    }

    pub fn update_delta(
        &self,
        prev: &FieldVector,
        old_node: &DesignNode,
        new_node: &DesignNode,
    ) -> FieldVector {
        let old_proj = self.project_node(old_node);
        let new_proj = self.project_node(new_node);

        let base = if prev.dimensions() == self.dimensions {
            prev.clone()
        } else {
            FieldVector::zeros(self.dimensions)
        };

        base.sub(&old_proj).add(&new_proj)
    }
}

pub fn aggregate(nodes: &[DesignNode]) -> FieldVector {
    let projector = HybridProjector::default_coefficients(64);
    aggregate_with_projector(nodes, &projector)
}

pub fn aggregate_with_projector(
    nodes: &[DesignNode],
    projector: &dyn NodeProjector,
) -> FieldVector {
    if nodes.is_empty() {
        return FieldVector::zeros(1);
    }

    let dim = projector.project(&nodes[0]).dimensions();
    let mut acc = FieldVector::zeros(dim);

    for (idx, node) in nodes.iter().enumerate() {
        let weight = 1.0f32 / (idx as f32 + 1.0);
        let p = projector.project(node).scale(weight);
        acc = acc.add(&p);
    }

    acc
}

pub fn resonance_score(field: &FieldVector, target: &TargetField) -> f64 {
    let len = field.dimensions().min(target.data.dimensions());
    if len == 0 {
        return 0.0;
    }

    let mut dot = Complex::new(0.0f32, 0.0f32);
    let mut norm_f = 0.0f32;
    let mut norm_t = 0.0f32;

    for i in 0..len {
        let f = field.data[i];
        let t = target.data.data[i];
        dot += f * t.conj();
        norm_f += f.norm_sqr();
        norm_t += t.norm_sqr();
    }

    if norm_f <= f32::EPSILON || norm_t <= f32::EPSILON {
        return 0.0;
    }

    let denom = norm_f.sqrt() * norm_t.sqrt();
    (dot.norm() / denom).clamp(0.0, 1.0) as f64
}

fn build_category_basis(dim: usize, category_seed: u64) -> FieldVector {
    let mut data = Vec::with_capacity(dim);
    for i in 0..dim {
        let x = splitmix64(category_seed.wrapping_add((i as u64).wrapping_mul(0xA24BAED4963EE407)));
        let re = if (x & 0b01) == 0 { 1.0 } else { -1.0 };
        let im = if (x & 0b10) == 0 { 1.0 } else { -1.0 };
        data.push(Complex::new(re, im));
    }
    FieldVector { data }
}

fn infer_category(node: &DesignNode) -> NodeCategory {
    if let Some(Value::Text(raw)) = node.attributes.get("category") {
        return parse_category(raw)
            .unwrap_or_else(|| parse_category(&node.kind).unwrap_or(NodeCategory::Abstraction));
    }
    parse_category(&node.kind).unwrap_or(NodeCategory::Abstraction)
}

fn parse_category(text: &str) -> Option<NodeCategory> {
    match text.to_ascii_lowercase().as_str() {
        "interface" => Some(NodeCategory::Interface),
        "storage" => Some(NodeCategory::Storage),
        "network" => Some(NodeCategory::Network),
        "compute" => Some(NodeCategory::Compute),
        "control" => Some(NodeCategory::Control),
        "constraint" => Some(NodeCategory::Constraint),
        "abstraction" => Some(NodeCategory::Abstraction),
        "performance" => Some(NodeCategory::Performance),
        "reliability" => Some(NodeCategory::Reliability),
        "costsensitive" | "cost_sensitive" | "cost-sensitive" => Some(NodeCategory::CostSensitive),
        _ => None,
    }
}

fn stable_hash_node(node: &DesignNode) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    h = fnv_u64(h, &node.id.as_u128().to_le_bytes());
    h = fnv_u64(h, node.kind.as_bytes());

    for (k, v) in &node.attributes {
        h = fnv_u64(h, k.as_bytes());
        h = fnv_u64(h, value_bytes(v).as_slice());
    }

    h
}

fn value_bytes(value: &Value) -> Vec<u8> {
    match value {
        Value::Int(v) => {
            let mut out = vec![0x01];
            out.extend_from_slice(&v.to_le_bytes());
            out
        }
        Value::Float(v) => {
            let mut out = vec![0x02];
            out.extend_from_slice(&v.to_bits().to_le_bytes());
            out
        }
        Value::Bool(v) => vec![0x03, u8::from(*v)],
        Value::Text(v) => {
            let mut out = vec![0x04];
            out.extend_from_slice(v.as_bytes());
            out
        }
    }
}

fn fnv_u64(mut state: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        state ^= *b as u64;
        state = state.wrapping_mul(0x100000001b3);
    }
    state
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use memory_space::{DesignNode, DesignState, StructuralGraph, Uuid, Value};

    use crate::{
        FieldEngine, HybridProjector, NodeCategory, NodeProjector, TargetField, resonance_score,
    };

    #[test]
    fn hybrid_projection_is_deterministic() {
        let mut attrs = BTreeMap::new();
        attrs.insert("category".to_string(), Value::Text("Network".to_string()));
        attrs.insert("key".to_string(), Value::Int(1));

        let node = DesignNode::new(Uuid::from_u128(1), "Network", attrs);
        let projector = HybridProjector::default_coefficients(64);

        let p1 = projector.project(&node);
        let p2 = projector.project(&node);
        assert_eq!(p1, p2);
    }

    #[test]
    fn category_bias_changes_projection() {
        let mut attrs_net = BTreeMap::new();
        attrs_net.insert("category".to_string(), Value::Text("Network".to_string()));

        let mut attrs_store = BTreeMap::new();
        attrs_store.insert("category".to_string(), Value::Text("Storage".to_string()));

        let node_a = DesignNode::new(Uuid::from_u128(1), "Network", attrs_net);
        let node_b = DesignNode::new(Uuid::from_u128(1), "Storage", attrs_store);

        let projector = HybridProjector::new(64, 0.8, 0.2);
        let pa = projector.project(&node_a);
        let pb = projector.project(&node_b);

        assert_ne!(pa, pb);
    }

    #[test]
    fn delta_update_runs() {
        let engine = FieldEngine::new(16);

        let mut attrs_old = BTreeMap::new();
        attrs_old.insert("x".to_string(), Value::Int(1));
        attrs_old.insert("category".to_string(), Value::Text("Compute".to_string()));
        let old = DesignNode::new(Uuid::from_u128(10), "Compute", attrs_old);

        let mut attrs_new = BTreeMap::new();
        attrs_new.insert("x".to_string(), Value::Int(2));
        attrs_new.insert("category".to_string(), Value::Text("Compute".to_string()));
        let new = DesignNode::new(Uuid::from_u128(10), "Compute", attrs_new);

        let prev = engine.project_node(&old);
        let updated = engine.update_delta(&prev, &old, &new);

        assert_eq!(updated.dimensions(), 16);
    }

    #[test]
    fn resonance_is_stable_and_bounded() {
        let mut graph = StructuralGraph::default();
        let mut attrs = BTreeMap::new();
        attrs.insert(
            "category".to_string(),
            Value::Text("Reliability".to_string()),
        );
        graph = graph.with_node_added(DesignNode::new(Uuid::from_u128(1), "Reliability", attrs));

        let state = DesignState::new(Uuid::from_u128(9), Arc::new(graph), "history:");
        let engine = FieldEngine::new(16);
        let f = engine.aggregate_state(&state);
        let t = TargetField::fixed(16);

        let r1 = resonance_score(&f, &t);
        let r2 = resonance_score(&f, &t);

        assert_eq!(r1, r2);
        assert!((0.0..=1.0).contains(&r1));
    }

    #[test]
    fn category_enum_is_usable() {
        let c = NodeCategory::Interface;
        assert_eq!(c.index(), 0);
    }
}
