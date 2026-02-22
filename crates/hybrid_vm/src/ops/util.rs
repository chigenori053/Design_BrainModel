use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use semantic_dhm::ConceptId;

use memory_space::InterferenceMode;

pub(crate) fn infer_depth_from_snapshot(snapshot: &str) -> usize {
    let Some(raw) = snapshot.strip_prefix("history:") else {
        return 0;
    };
    raw.split(',').filter(|part| !part.is_empty()).count()
}

pub(crate) fn default_store_path() -> PathBuf {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("hybrid_vm_store_{}_{}.bin", std::process::id(), id))
}

pub(crate) fn default_language_store_path() -> PathBuf {
    std::env::temp_dir().join("hybrid_vm_language_dhm.bin")
}

pub(crate) fn default_semantic_store_path() -> PathBuf {
    std::env::temp_dir().join("hybrid_vm_semantic_dhm.bin")
}

pub(crate) fn default_l1_store_path() -> PathBuf {
    std::env::temp_dir().join("hybrid_vm_semantic_l1_dhm.bin")
}

pub(crate) fn dot_norm(a: &[f32], b: &[f32]) -> f32 {
    let an = normalize(a);
    let bn = normalize(b);
    an.iter().zip(bn.iter()).map(|(l, r)| l * r).sum::<f32>()
}

fn normalize(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm <= f32::EPSILON {
        return vec![0.0; v.len()];
    }
    v.iter().map(|x| x / norm).collect()
}

pub(crate) fn dedup_ids(ids: &[ConceptId]) -> Vec<ConceptId> {
    let mut out = Vec::with_capacity(ids.len());
    let mut seen = HashSet::with_capacity(ids.len());
    for id in ids {
        if seen.insert(*id) {
            out.push(*id);
        }
    }
    out
}

pub(crate) fn memory_mode_from_env() -> InterferenceMode {
    let raw = std::env::var("PHASE6_MEMORY_MODE").unwrap_or_else(|_| "v6.1".to_string());
    match raw.to_ascii_lowercase().as_str() {
        "off" | "disabled" | "a" => InterferenceMode::Disabled,
        "v6.0" | "v6_0" | "contractive" | "b" => InterferenceMode::Contractive,
        _ => InterferenceMode::Repulsive,
    }
}
