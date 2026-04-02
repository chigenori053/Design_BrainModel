use std::fs;
use std::path::Path;

fn read(path: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(path)).expect("read file")
}

#[test]
fn no_web_launcher_symbol_remains() {
    let launcher = read("src/viewer/launcher.rs");
    let app = read("src/app.rs");
    let mod_file = read("src/viewer/mod.rs");
    let workspace = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root")
            .join("Cargo.toml"),
    )
    .expect("read workspace cargo");

    assert!(!launcher.contains("launch_viewer("));
    assert!(!launcher.contains("http://localhost:4173"));
    assert!(!launcher.contains("xdg-open"));
    assert!(!launcher.contains("open\")"));
    assert!(!app.contains("launch_viewer("));
    assert!(!mod_file.contains("launch_viewer"));
    assert!(!workspace.contains("\"apps/viewer\""));
    assert!(workspace.contains("\"apps/viewer_gui\""));
}
