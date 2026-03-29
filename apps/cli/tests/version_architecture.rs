use std::fs;
use std::path::Path;
use std::process::Command;

#[test]
fn manifest_is_single_bin_design_cli() {
    let manifest = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("Cargo.toml"),
    )
    .expect("read Cargo.toml");

    assert_eq!(manifest.matches("[[bin]]").count(), 1);
    assert!(manifest.contains("name = \"design_cli\""));
    assert!(manifest.contains("path = \"src/main.rs\""));
    for legacy in ["cli", "dbm", "design", "phase1_batch", "memory_admin"] {
        assert!(
            !manifest.contains(&format!("name = \"{legacy}\"")),
            "legacy bin should be removed from manifest: {legacy}"
        );
    }
}

#[test]
fn version_sources_use_cargo_pkg_version() {
    for rel in [
        "src/main.rs",
        "src/design_main.rs",
        "src/memory_admin_main.rs",
        "src/phase1_batch.rs",
        "src/ui/banner.rs",
    ] {
        let body = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(rel))
            .unwrap_or_else(|err| panic!("failed to read {rel}: {err}"));
        assert!(
            body.contains("CARGO_PKG_VERSION"),
            "{rel} must use env!(\"CARGO_PKG_VERSION\")"
        );
    }
}

#[test]
fn binary_version_matches_cargo_toml() {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .arg("--version")
        .output()
        .expect("run design_cli --version");
    assert!(out.status.success());

    let manifest = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("Cargo.toml"),
    )
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
