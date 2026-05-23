use std::fs;
use std::path::Path;
use std::process::Command;

fn run(args: &[&str]) -> (bool, String, String) {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args(args)
        .output()
        .expect("run design_cli");
    let stdout = String::from_utf8(out.stdout).expect("utf8 stdout");
    let stderr = String::from_utf8(out.stderr).expect("utf8 stderr");
    (out.status.success(), stdout, stderr)
}

#[test]
fn onboarding_help_surface_exposes_product_commands_and_matches_snapshot() {
    let (ok, stdout, stderr) = run(&["--help"]);
    assert!(ok, "stderr: {stderr}");

    for command in ["coding", "structure", "repl", "rules", "run", "git"] {
        assert!(stdout.contains(command), "missing {command} in {stdout}");
    }

    let snapshot_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("snapshots")
        .join("help_surface.txt");
    let expected = fs::read_to_string(snapshot_path).expect("read help snapshot");
    assert_eq!(stdout, expected);
}
