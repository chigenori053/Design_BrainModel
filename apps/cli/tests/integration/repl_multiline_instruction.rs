use std::fs;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_project(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_multiline_{name}_{unique}"));
    fs::create_dir_all(dir.join("src")).expect("create src");
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"multiline\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write cargo");
    fs::write(dir.join("src/lib.rs"), "pub mod coding;\n").expect("write lib");
    fs::write(dir.join("src/coding.rs"), "pub fn code() -> i32 { 0 }\n").expect("write coding");
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
fn long_spec_apply_requires_promote() {
    let dir = temp_project("reject");
    let input = "\
/begin spec
Target: src/coding.rs
Modify something
/end
apply
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("[APPLY] rejected: pending plan not promoted"),
        "stdout: {stdout}"
    );
}

#[test]
fn long_spec_promote_then_apply_is_allowed() {
    let dir = temp_project("allow");
    let input = "\
/begin spec
Target: src/coding.rs
Modify something
/end
promote
apply
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stdout.contains("[PROMOTE] preview:"), "stdout: {stdout}");
    // It should not be rejected by the guard.
    assert!(!stdout.contains("[APPLY] rejected:"), "stdout: {stdout}");
}

#[test]
fn target_only_apply_is_rejected() {
    let dir = temp_project("target_only");
    let input = "\
Target: src/coding.rs
apply
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("[APPLY] rejected: no preview transaction"),
        "stdout: {stdout}"
    );
}

#[test]
fn validate_plan_after_apply_runs_allowed_commands() {
    let dir = temp_project("validate_ok");
    let input = "\
/begin spec
Target: src/coding.rs
Add something
Validation: cargo check
/end
promote
apply
validate-plan
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("[VALIDATE] running: cargo check"),
        "stdout: {stdout}"
    );
    // We don't check for [VALIDATE] ok: here because it depends on the environment,
    // but the rejection check above should pass.
}

#[test]
fn validate_plan_rejects_before_apply() {
    let dir = temp_project("validate_before");
    let input = "\
/begin spec
Target: src/coding.rs
Add something
Validation: cargo check
/end
promote
validate-plan
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("[VALIDATE] rejected: no applied plan"),
        "stdout: {stdout}"
    );
}

#[test]
fn validate_plan_rejects_unsafe_command() {
    let dir = temp_project("validate_unsafe");
    let input = "\
/begin spec
Target: src/coding.rs
Add something
Validation: cargo check && rm -rf /
/end
promote
apply
validate-plan
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("[VALIDATE] rejected: unsafe validation command"),
        "stdout: {stdout}"
    );
}

#[test]
fn manual_e2e_risk_validation_not_in_operations() {
    let dir = temp_project("manual_plan");
    let input = "\
/begin spec
Target: src/coding.rs
REPL long instruction flow の手動E2E確認用コメントを追加する。
Risk: 不要な大規模変更をしない。
Validation: cargo test -p design_cli --test integration
/end
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("[PLAN] operation: InsertComment: REPL long instruction flow"),
        "stdout: {stdout}"
    );
    assert!(
        !stdout.contains("[PLAN] operation: Insert: Risk:"),
        "stdout: {stdout}"
    );
    assert!(
        !stdout.contains("[PLAN] operation: Insert: Validation:"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("[PLAN] risk: Risk: 不要な大規模変更をしない。"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("[PLAN] validate: Validation: cargo test -p design_cli --test integration"),
        "stdout: {stdout}"
    );
}

#[test]
fn manual_e2e_comment_instruction_does_not_generate_markers() {
    let dir = temp_project("manual_no_markers");
    let input = "\
/begin spec
Target: src/coding.rs
REPL long instruction flow の手動E2E確認用コメントを追加する。
Risk: 不要な大規模変更をしない。
Validation: cargo check
/end
promote
apply
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);
    assert_eq!(code, 0, "stderr: {stderr}");
    let content = fs::read_to_string(dir.join("src/coding.rs")).expect("read target");
    let combined = format!("{stdout}\n{content}");

    assert!(!combined.contains("REPL_RUNTIME_TEST"), "{combined}");
    assert!(!combined.contains("validate_runtime"), "{combined}");
    assert!(!combined.contains("test_marker"), "{combined}");
    assert!(!combined.contains("#[allow(dead_code)]"), "{combined}");
    assert!(
        content.contains("// REPL long instruction flow"),
        "content: {content}"
    );
}

#[test]
fn manual_e2e_apply_output_is_not_contradictory() {
    let dir = temp_project("manual_apply_output");
    let input = "\
/begin spec
Target: src/coding.rs
REPL long instruction flow の手動E2E確認用コメントを追加する。
Risk: 不要な大規模変更をしない。
Validation: cargo check
/end
promote
apply
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);
    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stdout.contains("transaction committed successfully"),
        "stdout: {stdout}"
    );
    assert!(
        !(stdout.contains("no active transaction")
            && stdout.contains("transaction committed successfully")),
        "stdout: {stdout}"
    );
}
