use std::fs;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_project(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_repl_stability_{name}_{unique}"));
    fs::create_dir_all(dir.join("src")).expect("create src");
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"repl_stability\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write cargo");
    fs::write(dir.join("src/lib.rs"), "pub fn run() {}\n").expect("write lib");
    dir
}

fn run_repl(dir: &std::path::Path, input: &str) -> (i32, String, String) {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let mut child = Command::new(exe)
        .arg("repl")
        .current_dir(dir)
        .env("DBM_VIEWER_SKIP_OPEN", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn repl");

    use std::io::Write;
    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(input.as_bytes())
        .expect("write repl input");

    let out = child.wait_with_output().expect("wait repl");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn repl_stability_normalization_and_shortcut() {
    let dir = temp_project("stability");

    // Test cases for normalization and "apply" shortcut
    // 1. "apply" shortcut
    // 2. trailing spaces and invisible characters
    let inputs = [
        "apply\n/exit\n",
        "apply  \n/exit\n",
        "coding --apply\n/exit\n",
    ];

    for input in inputs {
        let (code, stdout, stderr) = run_repl(&dir, input);
        assert_eq!(code, 0, "stderr: {stderr}");
        // Since there is no transaction, it should show "no previous coding transaction"
        // or "No changes to apply" (after my change if there was a transaction with no diff)
        // But here it will likely say "no previous coding transaction".
        assert!(
            stdout.contains("no previous coding transaction")
                || stdout.contains("No changes to apply"),
            "stdout: {stdout}"
        );
        assert!(
            stdout.contains("[planner: nl_v2] 1 steps"),
            "stdout: {stdout}"
        );
    }
}

#[test]
fn repl_stability_debug_log_present() {
    let dir = temp_project("debug_log");
    let (code, stdout, stderr) = run_repl(&dir, "analyze .\n/exit\n");
    assert_eq!(code, 0, "stderr: {stderr}");
    // Check if [DEBUG INPUT] is in stdout (since println! goes to stdout in this REPL setup if captured)
    // Actually run_repl captures stdout.
    assert!(
        stdout.contains("[DEBUG INPUT] \"analyze .\""),
        "stdout: {stdout}"
    );
}
