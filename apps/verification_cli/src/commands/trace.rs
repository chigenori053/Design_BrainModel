use std::path::Path;

use replay_engine::{capture, load_scenario};

use crate::error::CliResult;
use crate::io;

pub fn run(scenario: &str, output: &Path) -> CliResult<()> {
    let input = load_scenario(scenario)?;
    let trace = capture(&input)?;
    io::write_json(output, &trace)
}
