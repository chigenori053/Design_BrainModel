use std::fs;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_project(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_nl_repl_{name}_{unique}"));
    fs::create_dir_all(dir.join("src")).expect("create src");
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"nl_repl\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
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
fn repl_executes_natural_language_analysis_flow() {
    let dir = temp_project("analyze");
    let (code, stdout, stderr) = run_repl(&dir, "このプロジェクト全体を解析して\n/exit\n");
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("DBM >"), "stdout: {stdout}");
    assert!(stdout.contains("[planner: nl_v2] 1 steps"), "stdout: {stdout}");
    assert!(
        stdout.contains("design_cli analyze ."),
        "expected canonical analyze command in output: {stdout}"
    );
}

#[test]
fn repl_executes_natural_language_structure_flow() {
    let dir = temp_project("structure");
    let (code, stdout, stderr) = run_repl(&dir, "GUIで構造を開いて\n/exit\n");
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("[planner: nl_v2] 1 steps"), "stdout: {stdout}");
    assert!(
        stdout.contains("design_cli structure view ."),
        "expected canonical structure command in output: {stdout}"
    );
}
