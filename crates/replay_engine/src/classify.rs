/// Failure classification for diff reports (spec §8).
///
/// Maps the first mismatched layer to its root cause class so engineers
/// know exactly where non-determinism originates.
use serde::{Deserialize, Serialize};

use crate::diff::{LayerDiff, MatchStatus};

/// Root-cause classes for pipeline non-determinism (spec §8).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FailureClass {
    /// WebSearch returned different results → knowledge layer mismatch.
    ExternalNondeterminism,
    /// Memory retrieval ordering changed → memory layer mismatch.
    RetrievalInstability,
    /// Beam search tie-breaking is non-deterministic → search layer mismatch.
    SearchOrderingBug,
    /// Parser or normalization produced different IR → ir layer mismatch.
    IRGenerationBug,
    /// Code emit ordering changed → code layer mismatch.
    CodegenBug,
    /// Patch diff algorithm is non-deterministic → patch layer mismatch.
    PatchBug,
}

impl FailureClass {
    pub fn description(&self) -> &'static str {
        match self {
            Self::ExternalNondeterminism => {
                "ExternalNondeterminism — WebSearch produced different results. \
                 Ensure knowledge is snapshotted before replay (spec §12)."
            }
            Self::RetrievalInstability => {
                "RetrievalInstability — Memory retrieval ordering is non-deterministic. \
                 Use BTreeMap/BTreeSet in MemorySpace and add explicit tie-breaking."
            }
            Self::SearchOrderingBug => {
                "SearchOrderingBug — Beam search produced a different final beam. \
                 Check tie-breaking in ranking/pruning logic (use stable sort + deterministic keys)."
            }
            Self::IRGenerationBug => {
                "IRGenerationBug — Code IR differs between runs. \
                 Check parser normalization and module ordering."
            }
            Self::CodegenBug => {
                "CodegenBug — Generated code differs between runs. \
                 Check file emit ordering in codegen (use BTreeMap not HashMap)."
            }
            Self::PatchBug => {
                "PatchBug — Patch generation is non-deterministic. \
                 Verify the diff algorithm produces stable output."
            }
        }
    }
}

/// Classify a set of layer diffs into a root-cause FailureClass.
/// Returns the class corresponding to the first (highest-priority) mismatch.
pub fn classify_from_diffs(diffs: &[LayerDiff]) -> Option<FailureClass> {
    // Layer priority follows the pipeline order (spec §7).
    const PRIORITY: &[(&str, fn() -> FailureClass)] = &[
        ("knowledge", || FailureClass::ExternalNondeterminism),
        ("memory", || FailureClass::RetrievalInstability),
        ("search", || FailureClass::SearchOrderingBug),
        ("ir", || FailureClass::IRGenerationBug),
        ("code", || FailureClass::CodegenBug),
        ("patch", || FailureClass::PatchBug),
    ];

    for (layer_name, make_class) in PRIORITY {
        if let Some(diff) = diffs.iter().find(|d| d.layer == *layer_name) {
            if diff.match_status == MatchStatus::Mismatch {
                return Some(make_class());
            }
        }
    }
    None
}
