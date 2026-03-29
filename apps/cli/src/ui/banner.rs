pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn version() -> &'static str {
    VERSION
}

pub fn print_banner() {
    println!("DBM Design Brain Model v{}", VERSION);
}
