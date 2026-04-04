use serde::Serialize;
use viewer_core::benchmark::{ReplayBenchmarkReport, benchmark_replay};
use viewer_core::timeline::compact_delta_chain;

use super::attach_session;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BenchmarkCommandReport {
    pub root: String,
    pub snapshots: usize,
    pub compacted_snapshots: usize,
    pub report: ReplayBenchmarkReport,
}

pub fn benchmark_structure_replay(
    root: &std::path::Path,
) -> Result<BenchmarkCommandReport, String> {
    let session = attach_session(root)?;
    let core_snapshots = session
        .current_ir
        .snapshots
        .iter()
        .cloned()
        .map(super::StructureSnapshot::into_core)
        .collect::<Vec<_>>();
    let compacted = compact_delta_chain(&core_snapshots, 100);
    let report = benchmark_replay(&compacted);
    Ok(BenchmarkCommandReport {
        root: root.display().to_string(),
        snapshots: session.current_ir.snapshots.len(),
        compacted_snapshots: compacted.len(),
        report,
    })
}
