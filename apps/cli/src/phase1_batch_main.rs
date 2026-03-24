mod phase1_batch;
mod step0;

fn main() {
    ctrlc::set_handler(move || {
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Err(err) = phase1_batch::run_from_args(&args) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
