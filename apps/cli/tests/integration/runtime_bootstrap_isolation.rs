use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn repl_bootstrap_does_not_enter_nl_pipeline() {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let mut child = Command::new(exe)
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn repl");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"/exit\n")
        .expect("write exit");

    let output = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    assert!(output.status.success(), "stdout={stdout}\nstderr={stderr}");
    assert!(stdout.contains("DBM_CLI REPL"), "stdout={stdout}");
    assert!(stdout.contains("Type /exit to quit"), "stdout={stdout}");
    assert!(!combined.contains("[ROUTE]"), "{combined}");
    assert!(!combined.contains("stage=analyze"), "{combined}");
    assert!(!combined.contains("clarification"), "{combined}");
}
