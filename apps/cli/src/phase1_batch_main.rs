mod phase1_batch;
mod step0;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let Err(err) = phase1_batch::run_from_args(&args) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
