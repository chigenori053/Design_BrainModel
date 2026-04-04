use std::process::Command;

fn run(args: &[&str]) -> (i32, String, String) {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args(args)
        .output()
        .expect("run design_cli");
    let code = out.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    (code, stdout, stderr)
}

#[test]
fn coding_help_passes_through_to_app_surface() {
    let (_, stdout, stderr) = run(&["coding", "--help"]);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("Usage: design_cli coding"),
        "output: {combined}"
    );
    assert!(combined.contains("--check"), "output: {combined}");
}

#[test]
fn structure_help_passes_through_to_app_surface() {
    let (_, stdout, stderr) = run(&["structure", "--help"]);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("Usage: design_cli structure"),
        "output: {combined}"
    );
    assert!(combined.contains("view"), "output: {combined}");
}

#[test]
fn repl_help_passes_through_to_app_surface() {
    let (_, stdout, stderr) = run(&["repl", "--help"]);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("Usage: design_cli repl"),
        "output: {combined}"
    );
    assert!(combined.contains("--json"), "output: {combined}");
}
