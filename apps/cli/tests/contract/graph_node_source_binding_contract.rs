use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use design_cli::service::analyze_path;

fn temp_project(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_graph_binding_{name}_{unique}"));
    fs::create_dir_all(dir.join("src/runtime")).expect("runtime dir");
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"graph_binding\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(dir.join("src/lib.rs"), "pub mod runtime;\n").expect("lib");
    fs::write(
        dir.join("src/runtime/mod.rs"),
        "pub mod controller;\npub mod replay;\npub mod determinism;\n",
    )
    .expect("runtime mod");
    fs::write(
        dir.join("src/runtime/controller.rs"),
        "use crate::runtime::determinism;\npub fn run() { determinism::check(); }\n",
    )
    .expect("controller");
    fs::write(dir.join("src/runtime/replay.rs"), "pub fn replay() {}\n").expect("replay");
    fs::write(
        dir.join("src/runtime/determinism.rs"),
        "pub fn check() {}\n",
    )
    .expect("determinism");
    dir
}

#[test]
fn logical_determinism_node_binds_to_primary_source_path() {
    let root = temp_project("determinism");
    let report = analyze_path(&root).expect("analyze");
    let determinism = report
        .graph_nodes
        .iter()
        .find(|node| node.logical_name == "determinism")
        .expect("determinism graph node");
    assert_eq!(
        determinism
            .source_path
            .as_ref()
            .map(|path| path.display().to_string()),
        Some("src/runtime/determinism.rs".to_string())
    );
}
