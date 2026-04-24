use std::path::Path;

use replay_engine::{ExecutionTrace, classify, diff};

use crate::error::CliResult;
use crate::io;

pub fn run(trace1: &Path, trace2: &Path, output: Option<&Path>) -> CliResult<()> {
    let left: ExecutionTrace = io::read_json(trace1)?;
    let right: ExecutionTrace = io::read_json(trace2)?;
    let report = classify(diff(&left, &right)?)?;
    io::write_json_or_stdout(output, &report)
}
