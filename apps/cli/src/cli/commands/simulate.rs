use std::ffi::OsString;

use crate::ui::banner;

pub fn run(_args: Vec<OsString>) -> Result<(), String> {
    banner::print_banner();
    Err("legacy simulate is only available from the design_cli binary entrypoint".to_string())
}
