use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn binary_version_matches_cargo_toml() {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .arg("--version")
        .output()
        .expect("run design_cli --version");
    assert!(out.status.success());

    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
        .expect("read Cargo.toml");
    let manifest_version = manifest
        .lines()
        .find(|line| line.trim_start().starts_with("version = "))
        .expect("manifest version line")
        .split('"')
        .nth(1)
        .expect("quoted version");

    let stdout = String::from_utf8(out.stdout).expect("utf8");
    assert!(stdout.contains(manifest_version));
}
