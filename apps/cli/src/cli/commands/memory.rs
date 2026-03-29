use std::ffi::OsString;

use crate::memory_admin_main;

pub fn run(args: Vec<OsString>) -> Result<(), String> {
    let mut forwarded = vec![OsString::from("design_cli")];
    forwarded.extend(args);
    memory_admin_main::run_with_args(forwarded)
}
