use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn repl_target_only_does_not_mutate_file() {
    let root = tempfile::tempdir().expect("tempdir");
    let target = root.path().join("apps/cli/src/git_guard.rs");
    std::fs::create_dir_all(target.parent().expect("parent")).expect("mkdir");
    std::fs::write(&target, "fn guard() {}\n").expect("write");
    let before = std::fs::read_to_string(&target).expect("before");

    let exe = env!("CARGO_BIN_EXE_design_cli");
    let mut child = Command::new(exe)
        .arg("repl")
        .current_dir(root.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn repl");

    child
        .stdin
        .as_mut()
        .expect("stdin")
        .write_all(b"Target: apps/cli/src/git_guard.rs\n/exit\n")
        .expect("write input");

    let output = child.wait_with_output().expect("wait");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let after = std::fs::read_to_string(&target).expect("after");

    assert!(output.status.success(), "stdout={stdout}\nstderr={stderr}");
    assert_eq!(before, after);
    assert!(
        stdout.contains("[TARGET] context set: apps/cli/src/git_guard.rs"),
        "stdout={stdout}"
    );
    assert!(!stdout.contains("Applied"), "stdout={stdout}");
    assert!(!stdout.contains("[DIFF]"), "stdout={stdout}");
    assert!(!stdout.contains("preview ready"), "stdout={stdout}");
}
