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
fn subcommand_help_passes_through_to_app_surface() {
    for (subcommand, expected_usage, expected_detail) in [
        ("coding", "Usage: design_cli coding", "--check"),
        ("structure", "Usage: design_cli structure", "view"),
        ("repl", "Usage: design_cli repl", "--json"),
    ] {
        let (_, stdout, stderr) = run(&[subcommand, "--help"]);
        let combined = format!("{stdout}{stderr}");
        assert!(combined.contains(expected_usage), "output: {combined}");
        assert!(combined.contains(expected_detail), "output: {combined}");
    }
}
