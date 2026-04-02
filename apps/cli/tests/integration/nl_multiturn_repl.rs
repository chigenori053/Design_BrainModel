use std::fs;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_project(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_nl_multiturn_{name}_{unique}"));
    fs::create_dir_all(dir.join("src")).expect("create src");
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"nl_multiturn\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write cargo");
    fs::write(
        dir.join("src/lib.rs"),
        "pub mod presentation;\npub mod renderer;\npub fn run() {}\n",
    )
    .expect("write lib");
    fs::write(dir.join("src/presentation.rs"), "pub fn present() {}\n").expect("presentation");
    fs::write(dir.join("src/renderer.rs"), "pub fn render() {}\n").expect("renderer");
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
fn repl_multiturn_inherits_context_and_emits_git_dry_run_flow() {
    let dir = temp_project("conversation");
    let input = "\
この循環依存を見て
presentation layer 側だけ直して
GUIで差分を見せて
commitしてPR作って
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("[planner: nl]"), "stdout: {stdout}");
    assert!(stdout.contains("DBM[presentation] >"), "stdout: {stdout}");
    assert!(
        stdout.contains("design_cli structure dispatch . --event <generated diff>"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("design_cli execute \"commit changes\" --path . --dry-run --json"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("design_cli execute \"push and create pr\" --path . --dry-run --auto-remote --json"),
        "stdout: {stdout}"
    );
}
