/// Diff engine: layer-by-layer comparison of two FullTrace captures (spec §7).
use serde::{Deserialize, Serialize};

use crate::capture::hash_str;
use crate::classify::{classify_from_diffs, FailureClass};
use crate::trace::{FullTrace, MemoryLayerEntry, SearchLayerEntry};

// ── Output types ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiffReport {
    /// True when every layer matches exactly.
    pub deterministic: bool,
    pub layer_diffs: Vec<LayerDiff>,
    pub failure_class: Option<FailureClass>,
    pub summary: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayerDiff {
    pub layer: String,
    pub match_status: MatchStatus,
    pub details: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MatchStatus {
    Match,
    Mismatch,
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Compare `original` and `replayed` traces layer by layer.
/// Returns a DiffReport with per-layer match status and failure classification.
pub fn diff(original: &FullTrace, replayed: &FullTrace) -> DiffReport {
    let mut layer_diffs = vec![
        diff_input(original, replayed),
        diff_knowledge(original, replayed),
        diff_ir(original, replayed),
        diff_memory(&original.memory, &replayed.memory),
        diff_search(&original.search, &replayed.search),
        diff_code(&original.code, &replayed.code),
        diff_patch(original, replayed),
    ];

    let deterministic = layer_diffs
        .iter()
        .all(|d| d.match_status == MatchStatus::Match);

    let failure_class = if deterministic {
        None
    } else {
        classify_from_diffs(&layer_diffs)
    };

    let mismatch_count = layer_diffs
        .iter()
        .filter(|d| d.match_status == MatchStatus::Mismatch)
        .count();

    let summary = if deterministic {
        "All layers match — pipeline is fully deterministic.".into()
    } else {
        let classes: Vec<String> = layer_diffs
            .iter()
            .filter(|d| d.match_status == MatchStatus::Mismatch)
            .map(|d| d.layer.clone())
            .collect();
        format!(
            "{} layer(s) show non-determinism: {}",
            mismatch_count,
            classes.join(", ")
        )
    };

    // Append failure description to the relevant diff details.
    if let Some(ref class) = failure_class {
        if let Some(first_bad) = layer_diffs
            .iter_mut()
            .find(|d| d.match_status == MatchStatus::Mismatch)
        {
            first_bad
                .details
                .push(format!("Diagnosis: {}", class.description()));
        }
    }

    DiffReport {
        deterministic,
        layer_diffs,
        failure_class,
        summary,
    }
}

// ── Per-layer diff helpers ────────────────────────────────────────────────────

fn diff_input(orig: &FullTrace, rep: &FullTrace) -> LayerDiff {
    let mut details = Vec::new();

    if orig.input.initial_state_hash != rep.input.initial_state_hash {
        details.push(format!(
            "state_hash: {} → {}",
            orig.input.initial_state_hash, rep.input.initial_state_hash
        ));
    }
    if orig.input.state_id != rep.input.state_id {
        details.push(format!(
            "state_id: {} → {}",
            orig.input.state_id, rep.input.state_id
        ));
    }
    let orig_units: usize = orig
        .input
        .architecture
        .classes
        .iter()
        .flat_map(|c| c.structures.iter())
        .map(|s| s.units.len())
        .sum();
    let rep_units: usize = rep
        .input
        .architecture
        .classes
        .iter()
        .flat_map(|c| c.structures.iter())
        .map(|s| s.units.len())
        .sum();
    if orig_units != rep_units {
        details.push(format!("unit_count: {} → {}", orig_units, rep_units));
    }
    if orig.input.architecture.deps.len() != rep.input.architecture.deps.len() {
        details.push(format!(
            "dep_count: {} → {}",
            orig.input.architecture.deps.len(),
            rep.input.architecture.deps.len()
        ));
    }

    make_diff("input", details)
}

fn diff_knowledge(orig: &FullTrace, rep: &FullTrace) -> LayerDiff {
    let mut details = Vec::new();

    if orig.knowledge.content_hash != rep.knowledge.content_hash {
        details.push(format!(
            "content_hash: {} → {} (WebSearch non-determinism — spec §9.3)",
            orig.knowledge.content_hash, rep.knowledge.content_hash
        ));
    }
    if orig.knowledge.source_count != rep.knowledge.source_count {
        details.push(format!(
            "source_count: {} → {}",
            orig.knowledge.source_count, rep.knowledge.source_count
        ));
    }

    make_diff("knowledge", details)
}

fn diff_ir(orig: &FullTrace, rep: &FullTrace) -> LayerDiff {
    let mut details = Vec::new();

    if orig.ir.ir_hash != rep.ir.ir_hash {
        details.push(format!(
            "ir_hash: {} → {}",
            orig.ir.ir_hash, rep.ir.ir_hash
        ));
    }
    if orig.ir.module_count != rep.ir.module_count {
        details.push(format!(
            "module_count: {} → {}",
            orig.ir.module_count, rep.ir.module_count
        ));
    }
    if orig.ir.dependency_count != rep.ir.dependency_count {
        details.push(format!(
            "dependency_count: {} → {}",
            orig.ir.dependency_count, rep.ir.dependency_count
        ));
    }

    make_diff("ir", details)
}

fn diff_memory(orig: &[MemoryLayerEntry], rep: &[MemoryLayerEntry]) -> LayerDiff {
    let mut details = Vec::new();

    if orig.len() != rep.len() {
        details.push(format!("pattern_count: {} → {}", orig.len(), rep.len()));
    }

    // Compare pattern IDs in order — ordering matters for retrieval determinism.
    let id_mismatches: Vec<_> = orig
        .iter()
        .zip(rep.iter())
        .enumerate()
        .filter(|(_, (o, r))| o.pattern_id != r.pattern_id)
        .map(|(i, (o, r))| format!("  memory[{}]: id {} → {}", i, o.pattern_id, r.pattern_id))
        .collect();
    if !id_mismatches.is_empty() {
        details.push(format!(
            "{} pattern ordering mismatch(es):",
            id_mismatches.len()
        ));
        details.extend(id_mismatches);
    }

    make_diff("memory", details)
}

fn diff_search(orig: &[SearchLayerEntry], rep: &[SearchLayerEntry]) -> LayerDiff {
    let mut details = Vec::new();

    if orig.len() != rep.len() {
        details.push(format!("beam_size: {} → {}", orig.len(), rep.len()));
    }

    if let (Some(o), Some(r)) = (orig.first(), rep.first()) {
        if o.state_hash != r.state_hash {
            details.push(format!(
                "top_state_hash: {} → {}",
                o.state_hash, r.state_hash
            ));
        }
        let delta = (o.score - r.score).abs();
        if delta > 1e-9 {
            details.push(format!("top_score_delta: {:.2e}", delta));
        }
    }

    let hash_mismatches: Vec<_> = orig
        .iter()
        .zip(rep.iter())
        .enumerate()
        .filter(|(_, (o, r))| o.state_hash != r.state_hash)
        .map(|(i, (o, r))| {
            format!(
                "  beam[{}]: {} → {} (score {:.4} → {:.4})",
                i, o.state_hash, r.state_hash, o.score, r.score
            )
        })
        .collect();
    if !hash_mismatches.is_empty() {
        details.push(format!("{} beam state mismatch(es):", hash_mismatches.len()));
        details.extend(hash_mismatches);
    }

    make_diff("search", details)
}

fn diff_code(orig: &str, rep: &str) -> LayerDiff {
    let orig_hash = hash_str(orig);
    let rep_hash = hash_str(rep);
    let details = if orig_hash != rep_hash {
        vec![format!("code_hash: {} → {}", orig_hash, rep_hash)]
    } else {
        vec![]
    };
    make_diff("code", details)
}

fn diff_patch(orig: &FullTrace, rep: &FullTrace) -> LayerDiff {
    let mut details = Vec::new();
    if orig.patch.len() != rep.patch.len() {
        details.push(format!(
            "patch_count: {} → {}",
            orig.patch.len(),
            rep.patch.len()
        ));
    }
    make_diff("patch", details)
}

// ── Utility ───────────────────────────────────────────────────────────────────

fn make_diff(layer: &str, details: Vec<String>) -> LayerDiff {
    LayerDiff {
        layer: layer.into(),
        match_status: if details.is_empty() {
            MatchStatus::Match
        } else {
            MatchStatus::Mismatch
        },
        details,
    }
}
