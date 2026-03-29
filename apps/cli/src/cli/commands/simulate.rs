use std::ffi::OsString;

use crate::design_main;
use crate::ui::banner;

pub fn run(args: Vec<OsString>) -> Result<(), String> {
    banner::print_banner();

    let mut forwarded = vec![OsString::from("design_cli"), OsString::from("simulate")];
    forwarded.extend(args);
    design_main::run_with_args(forwarded)
}
