use std::ffi::OsString;

fn main() {
    let args = std::env::args_os().collect::<Vec<OsString>>();
    if let Err(err) = design_cli::verify_cli::run_with_args(args) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
