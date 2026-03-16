use std::collections::BTreeMap;

use architecture_ir::{ArchitectureIR, ComponentType, architecture_hash};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArchitectureMetadata {
    pub search_depth: usize,
    pub generation_time: u64,
    pub search_iteration: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArchitectureRecord {
    pub architecture_id: String,
    pub architecture_ir: ArchitectureIR,
    pub template_origin: String,
    pub evaluation_score: f32,
    pub metadata: ArchitectureMetadata,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ArchitectureMemoryDomain {
    records: BTreeMap<String, ArchitectureRecord>,
}

impl ArchitectureMemoryDomain {
    pub fn upsert(&mut self, record: ArchitectureRecord) {
        self.records.insert(record.architecture_id.clone(), record);
    }

    pub fn get(&self, architecture_id: &str) -> Option<&ArchitectureRecord> {
        self.records.get(architecture_id)
    }

    pub fn all(&self) -> Vec<&ArchitectureRecord> {
        self.records.values().collect()
    }

    pub fn find_similar(&self, architecture: &ArchitectureIR, top_k: usize) -> Vec<ArchitectureRecord> {
        let mut scored = self
            .records
            .values()
            .cloned()
            .map(|record| (similarity_score(architecture, &record.architecture_ir), record))
            .collect::<Vec<_>>();
        scored.sort_by(|(ls, la), (rs, ra)| {
            rs.total_cmp(ls)
                .then_with(|| la.architecture_id.cmp(&ra.architecture_id))
        });
        scored.into_iter().take(top_k.max(1)).map(|(_, record)| record).collect()
    }

    pub fn find_by_structural_hash(&self, architecture: &ArchitectureIR) -> Option<&ArchitectureRecord> {
        let hash = architecture_hash_string(architecture);
        self.records
            .values()
            .find(|record| architecture_hash_string(&record.architecture_ir) == hash)
    }
}

pub fn architecture_hash_string(architecture: &ArchitectureIR) -> String {
    format!("{:016x}", architecture_hash(architecture))
}

fn similarity_score(lhs: &ArchitectureIR, rhs: &ArchitectureIR) -> f32 {
    let lhs_components = lhs.components.len() as f32;
    let rhs_components = rhs.components.len() as f32;
    let lhs_edges = lhs.dependencies.len() as f32;
    let rhs_edges = rhs.dependencies.len() as f32;
    let component_delta = (lhs_components - rhs_components).abs();
    let edge_delta = (lhs_edges - rhs_edges).abs();
    let component_types = overlap_component_types(lhs, rhs);
    let exact_hash = if architecture_hash(lhs) == architecture_hash(rhs) {
        1.0
    } else {
        0.0
    };
    (exact_hash * 0.5
        + component_types * 0.3
        + inverse_delta(component_delta) * 0.1
        + inverse_delta(edge_delta) * 0.1)
        .clamp(0.0, 1.0)
}

fn overlap_component_types(lhs: &ArchitectureIR, rhs: &ArchitectureIR) -> f32 {
    let mut lhs_types = lhs
        .components
        .iter()
        .map(|component| component.component_type.clone())
        .collect::<Vec<ComponentType>>();
    lhs_types.sort_by_key(|kind| format!("{kind:?}"));
    lhs_types.dedup();

    let mut rhs_types = rhs
        .components
        .iter()
        .map(|component| component.component_type.clone())
        .collect::<Vec<ComponentType>>();
    rhs_types.sort_by_key(|kind| format!("{kind:?}"));
    rhs_types.dedup();

    let overlap = lhs_types.iter().filter(|kind| rhs_types.contains(kind)).count() as f32;
    let total = lhs_types.len().max(rhs_types.len()) as f32;
    if total == 0.0 {
        1.0
    } else {
        overlap / total
    }
}

fn inverse_delta(delta: f32) -> f32 {
    1.0 / (1.0 + delta.max(0.0))
}
