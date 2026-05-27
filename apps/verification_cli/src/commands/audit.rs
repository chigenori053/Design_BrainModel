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

pub fn run(scenario: &str, output: Option<&Path>) -> CliResult<()> {
    let input = load_scenario(scenario)?;
    let trace = capture(&input)?;
    let replayed = replay(&trace)?;
    let report = classify(diff(&trace, &replayed)?)?;
    if output.is_none() {
        return io::write_json_or_stdout(None, &report);
    }

    let audit = AuditReport {
        scenario: input.name,
        trace,
        replay: replayed,
        report,
    };
    io::write_json(output.expect("checked output"), &audit)
}
