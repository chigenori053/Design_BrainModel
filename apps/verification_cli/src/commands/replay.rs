use std::path::Path;

use replay_engine::{ExecutionTrace, replay};

use crate::error::CliResult;
use crate::io;

pub fn run(trace_path: &Path, output: &Path) -> CliResult<()> {
    let trace: ExecutionTrace = io::read_json(trace_path)?;
    let replayed = replay(&trace)?;
    io::write_json(output, &replayed)
}
