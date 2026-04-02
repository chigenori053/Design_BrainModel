use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use design_cli::planner::rule_based::RuleBasedPlanner;

fn temp_project(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_refactor_integration_{name}_{unique}"));
    fs::create_dir_all(dir.join("src")).expect("create src");
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"refactor_integration\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write cargo");
    fs::write(
        dir.join("src/lib.rs"),
        "pub mod renderer;\npub mod debug;\n",
    )
    .expect("write lib");
    fs::write(
        dir.join("src/renderer.rs"),
        "use crate::debug;\npub fn render() {}\n",
    )
    .expect("write renderer");
    fs::write(
        dir.join("src/debug.rs"),
        "use crate::renderer;\npub fn debug() {}\n",
    )
    .expect("write debug");
    dir
}

fn run(args: &[&str]) -> (i32, String, String) {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe).args(args).output().expect("run design");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

#[test]
fn analyze_to_refactor_preview_works() {
    let dir = temp_project("preview");
    let (code, stdout, _) = run(&[
        "refactor",
        dir.to_str().unwrap(),
        "--target",
        "cycle",
        "--json",
    ]);
    assert_eq!(code, 0);
    assert!(stdout.contains("\"preview\""));
    assert!(stdout.contains("\"cli_text_preview\""));
}

#[test]
fn nl_cycle_break_reaches_safe_apply_command() {
    let planner = RuleBasedPlanner::new();
    let dir = temp_project("nl_apply");
    let plan = planner.plan("循環を切る", Some(dir.to_str().unwrap()));
    let command = plan.steps[0].command.as_ref().expect("command");
    assert_eq!(command.name, "refactoring");
}
