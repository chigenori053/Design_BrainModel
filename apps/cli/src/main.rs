fn main() {
    if let Err(err) = design_cli::app::run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
