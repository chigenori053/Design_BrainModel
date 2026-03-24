fn main() {
    ctrlc::set_handler(move || {
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    if let Err(err) = design_cli::app::run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
