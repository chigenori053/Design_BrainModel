use std::ffi::OsString;

use crate::phase1_batch;

pub fn run(args: Vec<OsString>) -> Result<(), String> {
    let forwarded = args
        .into_iter()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    phase1_batch::run_from_args(&forwarded)
}
