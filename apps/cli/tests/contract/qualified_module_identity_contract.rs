use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use design_cli::source_index::ModuleSourceIndex;

fn temp_workspace(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_contract_{name}_{unique}"));
    fs::create_dir_all(dir.join("apps/cli/src")).expect("cli src");
    fs::create_dir_all(dir.join("tests/fixtures/architecture_clean/src/app")).expect("fixture src");
    fs::write(dir.join("apps/cli/src/app.rs"), "fn app() {}\n").expect("app");
    fs::write(
        dir.join("tests/fixtures/architecture_clean/src/app/mod.rs"),
        "fn fixture_app() {}\n",
    )
    .expect("fixture app");
    dir
}

#[test]
fn qualified_module_identity_prefers_production_crate_and_excludes_fixtures() {
    let root = temp_workspace("qualified_identity");
    let index = ModuleSourceIndex::build(&root).expect("index");

    assert_eq!(
        index.resolve("cli::app").expect("resolve cli::app"),
        Some(std::path::PathBuf::from("apps/cli/src/app.rs"))
    );
    assert_eq!(
        index.resolve("app").expect("resolve app"),
        Some(std::path::PathBuf::from("apps/cli/src/app.rs"))
    );
}
