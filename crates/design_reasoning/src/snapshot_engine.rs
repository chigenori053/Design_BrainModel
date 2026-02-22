use semantic_dhm::{
    ConceptUnit, MeaningLayerSnapshot, MeaningLayerState, SemanticError, SemanticUnitL1,
    SnapshotDiff, Snapshotable, compare_snapshots,
};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

const SNAPSHOT_V2_VERSION: u16 = 2;
// FNV-1a 64bit fixed parameters (seed/prime are immutable by policy).
const FNV_OFFSET_BASIS_64: u64 = 0xcbf29ce484222325;
const FNV_PRIME_64: u64 = 0x100000001b3;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeaningLayerSnapshotV2 {
    pub l1_hash: u64,
    pub l2_hash: u64,
    pub timestamp_ms: u64,
    pub version: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotDiffV2 {
    pub identical: bool,
    pub l1_changed: bool,
    pub l2_changed: bool,
    pub version_changed: bool,
}

#[derive(Clone, Default)]
pub struct SnapshotEngine;

impl SnapshotEngine {
    pub fn snapshot(
        &self,
        algorithm_version: u32,
        l1_units: Vec<SemanticUnitL1>,
        l2_units: Vec<ConceptUnit>,
    ) -> Result<MeaningLayerSnapshot, SemanticError> {
        Ok(MeaningLayerState {
            algorithm_version,
            l1_units,
            l2_units,
        }
        .snapshot())
    }

    pub fn compare(
        &self,
        a: &MeaningLayerSnapshot,
        b: &MeaningLayerSnapshot,
    ) -> Result<SnapshotDiff, SemanticError> {
        compare_snapshots(a, b)
    }

    pub fn make_snapshot_v2(
        &self,
        l1_units: &[SemanticUnitL1],
        l2_units: &[ConceptUnit],
    ) -> Result<MeaningLayerSnapshotV2, SemanticError> {
        Ok(MeaningLayerSnapshotV2 {
            l1_hash: hash_l1_units(l1_units),
            l2_hash: hash_l2_units(l2_units),
            timestamp_ms: now_timestamp_ms()?,
            version: SNAPSHOT_V2_VERSION,
        })
    }

    pub fn compare_snapshots_v2(
        &self,
        a: &MeaningLayerSnapshotV2,
        b: &MeaningLayerSnapshotV2,
    ) -> SnapshotDiffV2 {
        SnapshotDiffV2 {
            identical: a.l1_hash == b.l1_hash && a.l2_hash == b.l2_hash && a.version == b.version,
            l1_changed: a.l1_hash != b.l1_hash,
            l2_changed: a.l2_hash != b.l2_hash,
            version_changed: a.version != b.version,
        }
    }
}

fn now_timestamp_ms() -> Result<u64, SemanticError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| SemanticError::SnapshotError(e.to_string()))?
        .as_millis() as u64)
}

fn hash_l1_units(l1_units: &[SemanticUnitL1]) -> u64 {
    let mut canonical = l1_units
        .iter()
        .map(canonicalize_l1)
        .collect::<Vec<_>>();
    canonical.sort();
    hash_sorted(&canonical)
}

fn hash_l2_units(l2_units: &[ConceptUnit]) -> u64 {
    let mut canonical = l2_units
        .iter()
        .map(canonicalize_l2)
        .collect::<Vec<_>>();
    canonical.sort();
    hash_sorted(&canonical)
}

fn hash_sorted(values: &[String]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS_64;
    for value in values {
        // Values are UTF-8 normalized strings and hashed in deterministic byte order.
        hash = fnv1a64(hash, value.as_bytes());
        hash = fnv1a64(hash, b"|");
    }
    hash
}

fn fnv1a64(mut hash: u64, bytes: &[u8]) -> u64 {
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(FNV_PRIME_64);
    }
    hash
}

fn canonicalize_l1(unit: &SemanticUnitL1) -> String {
    let source = canonicalize_text(&unit.source_text);
    let vector = canonicalize_f32_vec(&unit.vector);
    let source_opt = canonicalize_option(Some(source));
    format!(
        "{}|{:?}|{}|{:.6}|{}|{}",
        unit.id.0,
        unit.role,
        unit.polarity.clamp(-1, 1),
        unit.abstraction.clamp(0.0, 1.0),
        source_opt,
        vector
    )
}

fn canonicalize_l2(unit: &ConceptUnit) -> String {
    let mut refs = unit.l1_refs.iter().map(|id| id.0).collect::<Vec<_>>();
    refs.sort();
    let refs_str = refs
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{}|{}|{:.6}|{}|{}|{}",
        unit.id.0,
        refs_str,
        unit.a.clamp(0.0, 1.0),
        canonicalize_f32_vec(&unit.integrated_vector),
        canonicalize_f32_vec(&unit.s),
        unit.polarity.clamp(-1, 1)
    )
}

fn canonicalize_f32_vec(values: &[f32]) -> String {
    values
        .iter()
        .map(|v| format!("{:.6}", v))
        .collect::<Vec<_>>()
        .join(",")
}

fn canonicalize_text(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn canonicalize_option(value: Option<String>) -> String {
    match value {
        Some(v) => format!("some:{v}"),
        None => "null".to_string(),
    }
}
