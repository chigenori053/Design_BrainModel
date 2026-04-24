use std::path::Path;

use replay_engine::{DiffReport, ExecutionTrace, capture, classify, diff, load_scenario, replay};
use serde::Serialize;

use crate::error::CliResult;
use crate::io;

#[derive(Debug, Serialize)]
struct AuditReport {
    scenario: String,
    trace: ExecutionTrace,
    replay: ExecutionTrace,
    report: DiffReport,
}

pub fn run(scenario: &str, output: &Path) -> CliResult<()> {
    let input = load_scenario(scenario)?;
    let trace = capture(&input)?;
    let replayed = replay(&trace)?;
    let report = classify(diff(&trace, &replayed)?)?;
    let audit = AuditReport {
        scenario: input.name,
        trace,
        replay: replayed,
        report,
    };
    io::write_json(output, &audit)
}
