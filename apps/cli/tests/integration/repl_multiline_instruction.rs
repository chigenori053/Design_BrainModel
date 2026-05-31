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

// CATEGORY: E2E
/// Critical User Flow: Plan -> Promote -> Apply -> Validate
#[test]
fn critical_user_flow_happy_path() {
    let dir = temp_project("critical_flow");
    let input = "\
/begin spec
Target: src/coding.rs
Modify code
Validation: cargo check
/end
promote
apply
validate-plan
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);
    assert_eq!(code, 0, "stderr: {stderr}");

    // 1. Plan & Promote
    assert!(stdout.contains("[PLAN]"), "Should contain plan: {stdout}");
    assert!(
        stdout.contains("[PROMOTE] preview:"),
        "Should contain preview: {stdout}"
    );

    // 2. Apply
    assert!(
        stdout.contains("transaction committed successfully"),
        "Should apply: {stdout}"
    );

    // 3. Validate
    assert!(
        stdout.contains("[VALIDATE] running: cargo check"),
        "Should run validation: {stdout}"
    );
}

// CATEGORY: E2E
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
        stdout.contains("[PLAN] risk: Risk: 不要な大規模変更をしない。"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("[PLAN] validate: Validation: cargo test -p design_cli --test integration"),
        "stdout: {stdout}"
    );
}

// CATEGORY: E2E
#[test]
fn specification_capture_end_boundary_dispatches_design_specification() {
    let dir = temp_project("spec_boundary_dispatch");
    let input = "/begin spec\r\nsystem_name: DesignBrainModel\r\ngoals:\r\nconstraints:\r\narchitecture:\r\nrules:\r\n  - ApplyGate required\r\n/end   \r\n/exit\r\n";
    let (code, stdout, stderr) = run_repl(&dir, input);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stderr.contains("[SPEC_END_TRACE]\nchecking_end_command"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("[SPEC_END_TRACE]\nend_command_matched"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("[SPEC_DISPATCH]"), "stderr: {stderr}");
    assert!(
        stderr.contains("[SPEC_CLASSIFIER]\nkind=DesignSpecification"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("[SPEC_CONTEXT]"), "stderr: {stderr}");
    assert!(
        stderr.contains("[STRUCTURAL_DIAGNOSIS]\nstatus=started"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("[STRUCTURAL_DIAGNOSIS]\nstatus=completed"),
        "stderr: {stderr}"
    );
    assert!(
        stdout.contains("[STRUCTURAL_DIAGNOSIS] violations=0 warnings=0"),
        "stdout: {stdout}"
    );
}

// CATEGORY: E2E
#[test]
fn design_specification_start_auto_captures_without_begin_spec() {
    let dir = temp_project("spec_auto_capture");
    let input = "\
system_name: DBM_REPL_UI
goals:
constraints:
architecture:
rules:
  - ApplyGate required
/end
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stderr.contains("[SPEC_RECOGNITION]\nchecking"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("[SPEC_RECOGNITION]\nkind=DesignSpecification\nreason=\"system_name\""),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("[SPEC_SESSION]") && stderr.contains("action=create"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("[SPEC_CONTEXT]"), "stderr: {stderr}");
    assert!(
        stderr.contains("[STRUCTURAL_DIAGNOSIS]\nstatus=completed"),
        "stderr: {stderr}"
    );
    assert!(
        stdout.contains("[SPEC_CONTEXT] generated")
            && stdout.contains("[STRUCTURAL_DIAGNOSIS] violations=0 warnings=0"),
        "stdout: {stdout}"
    );
}

// CATEGORY: E2E
#[test]
fn design_specification_diagnosis_runs_repair_planning() {
    let dir = temp_project("spec_repair_planning");
    let input = "\
system_name: DBM_REPL_UI
goals:
  - Separate input and output
constraints:
  - Preserve REPL compatibility
architecture:
  Runtime:
    responsibilities:
      - execution
rules:
  - Runtime must pass through AuditCore
/end
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(stderr.contains("[SPEC_CONTEXT]"), "stderr: {stderr}");
    assert!(
        stderr.contains("[STRUCTURAL_DIAGNOSIS]"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("[REPAIR_PLANNING]\nstatus=started"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("[REPAIR_SUGGESTION]"), "stderr: {stderr}");
    assert!(stderr.contains("[REPAIR_PLAN]"), "stderr: {stderr}");
    assert!(
        stderr.contains("[REPAIR_PLANNING]\nstatus=completed"),
        "stderr: {stderr}"
    );
    assert!(stdout.contains("Repair Suggestions:"), "stdout: {stdout}");
    assert!(stdout.contains("Enforce ApplyGate"), "stdout: {stdout}");
}

// CATEGORY: E2E
#[test]
fn specification_capture_invalid_end_boundary_does_not_dispatch() {
    let dir = temp_project("spec_boundary_invalid");
    let input = "\
/begin spec
system_name: DesignBrainModel
goals:
/end/end
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);

    assert_eq!(code, 0, "stderr: {stderr}");
    assert!(
        stderr.contains("[SPEC_END_TRACE]\nend_command_not_matched"),
        "stderr: {stderr}"
    );
    assert!(
        stdout.contains("[SPEC] rejected: invalid end command"),
        "stdout: {stdout}"
    );
    assert!(
        !stderr.contains("[SPEC_DISPATCH]"),
        "invalid end must not dispatch: {stderr}"
    );
}
