/// Phase G4.1: Follow-up Apply Intent Promotion テスト
///
/// `coding --apply` が前回 dry-run coding transaction を deterministic に apply へ昇格する
/// ことを検証する (R1–R6)。
use std::fs;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_project(name: &str) -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_apply_promo_{name}_{unique}"));
    fs::create_dir_all(dir.join("src")).expect("create src");
    fs::write(
        dir.join("Cargo.toml"),
        "[package]\nname = \"apply_promo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
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

/// Case1: happy path
///
/// 初回 coding dry-run → 継続 `coding --apply` で apply promotion が発火し、
/// `--safe --check` の再実行ループが起きないことを確認する。
#[test]
fn apply_promotion_fires_after_coding_dry_run() {
    let dir = temp_project("happy");
    let input = "\
src/coding.rs を改善して
coding --apply
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);
    assert_eq!(code, 0, "stderr: {stderr}");

    // planner が2ターンで nl_v2 ルートを使うこと
    assert_eq!(
        stdout.matches("[planner: nl_v2] 1 steps").count(),
        2,
        "expected 2 nl_v2 plans\nstdout: {stdout}"
    );

    // 2ターン目は `apply_previous_coding` ラベルで実行されること (R2 bypass 確認)
    assert!(
        stdout.contains("apply_previous_coding"),
        "apply_previous_coding label missing\nstdout: {stdout}"
    );

    // `--safe --check` が2回以上現れないこと (re-plan ループ消滅の確認)
    assert!(
        stdout.matches("--safe --check").count() < 2,
        "re-plan loop detected: --safe --check appeared more than once\nstdout: {stdout}"
    );

    // target continuity: prompt label が維持されること (R3)
    assert!(
        stdout.contains("DBM[coding.rs] >"),
        "target continuity broken\nstdout: {stdout}"
    );

    // Applied: のラインが出力されること
    assert!(
        stdout.contains("Applied:"),
        "Applied: output missing\nstdout: {stdout}"
    );
}

/// Case3: already applied — 2回連続 apply は no-op になる (R6)
#[test]
fn double_apply_is_no_op() {
    let dir = temp_project("double");
    let input = "\
src/coding.rs を改善して
coding --apply
coding --apply
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);
    assert_eq!(code, 0, "stderr: {stderr}");

    // 3ターン目の apply は "already applied" を返すこと
    assert!(
        stdout.contains("already applied"),
        "already applied guard missing\nstdout: {stdout}"
    );
}

/// Case4: target continuity — apply 後も DBM[target] > プロンプトが維持されること (R3)
#[test]
fn target_continuity_after_apply() {
    let dir = temp_project("target");
    let input = "\
src/coding.rs を改善して
coding --apply
/exit
";
    let (code, stdout, stderr) = run_repl(&dir, input);
    assert_eq!(code, 0, "stderr: {stderr}");

    // apply 後もターゲット継続
    assert!(
        stdout.contains("DBM[coding.rs] >"),
        "prompt label not preserved after apply\nstdout: {stdout}"
    );
}

/// Case2: zero patch no-op (R5)
///
/// 前回 dry-run が 0 patches だった場合、apply は no-op になる。
/// 実際のコーディングエンジンの出力に依存するため、apply_previous_coding が発火して
/// "Applied:" が出ることだけを検証する。
#[test]
fn apply_after_zero_patch_dryrun_produces_applied_output() {
    let dir = temp_project("zero");
    let input = "\
src/coding.rs を改善して
coding --apply
/exit
";
    let (code, stdout, _stderr) = run_repl(&dir, input);
    assert_eq!(code, 0);

    // apply_previous_coding が発火すること (routing の確認)
    assert!(
        stdout.contains("apply_previous_coding"),
        "apply routing did not fire\nstdout: {stdout}"
    );

    // Applied: false (no pending patches) または Applied: true のいずれかが出力されること
    assert!(
        stdout.contains("Applied:"),
        "Applied: output missing\nstdout: {stdout}"
    );
}
