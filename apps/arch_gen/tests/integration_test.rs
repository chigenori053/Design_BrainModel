//! arch_gen CLI 統合テスト
//!
//! `cargo test -p arch_gen --test integration_test` で実行。
//! バイナリを `cargo run -p arch_gen --bin arch_gen --` 経由で呼び出し、
//! stdout / stderr / exit code を検証する。

use std::path::PathBuf;
use std::process::Command;

// ─── ヘルパー ────────────────────────────────────────────────────────────────

/// `cargo run -p arch_gen --bin arch_gen -- <args>` を実行して (stdout, status) を返す。
fn arch_gen(args: &[&str]) -> (String, bool) {
    let mut cmd = Command::new("cargo");
    cmd.arg("run")
        .arg("--quiet")
        .arg("-p")
        .arg("arch_gen")
        .arg("--bin")
        .arg("arch_gen")
        .arg("--")
        .args(args)
        .current_dir(repo_root());

    let output = cmd.output().expect("failed to run arch_gen via cargo");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    (stdout, output.status.success())
}

/// ワークスペースルートを取得する。
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("workspace root not found")
        .to_path_buf()
}

// ─── generate コマンド ────────────────────────────────────────────────────────

#[test]
fn test_generate_basic() {
    let (stdout, success) = arch_gen(&[
        "/generate",
        "シンプルなWebアプリを設計する",
        "--no-code",
        "-n",
        "1",
    ]);
    assert!(
        success,
        "arch_gen /generate should exit 0\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("Design Conversation") || stdout.contains("案 1"),
        "stdout should contain conversational candidate output\n{stdout}"
    );
    assert!(
        stdout.contains("設計上のコメント"),
        "stdout should contain narrative review\n{stdout}"
    );
}

#[test]
fn test_generate_without_requirement_starts_conversation() {
    let (stdout, success) = arch_gen(&["/generate"]);
    assert!(
        success,
        "arch_gen /generate without requirement should start conversation"
    );
    assert!(
        stdout.contains("interactive mode") || stdout.contains("arch_gen interactive mode"),
        "stdout should contain conversation banner\n{stdout}"
    );
}

#[test]
fn test_generate_deterministic() {
    let args = &["/generate", "ECサイトを設計する", "--no-code", "-n", "1"];
    let (out1, ok1) = arch_gen(args);
    let (out2, ok2) = arch_gen(args);
    assert!(ok1 && ok2, "both runs should succeed");
    assert_eq!(out1, out2, "same input should produce identical output");
}

#[test]
fn test_generate_json_format() {
    let (stdout, success) = arch_gen(&[
        "/generate",
        "APIサーバを設計する",
        "--no-code",
        "-n",
        "1",
        "-f",
        "json",
    ]);
    assert!(success);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("output should be valid JSON");
    assert!(v["candidates"].is_array());
    assert!(v["input"].is_string());
}

#[test]
fn test_generate_mermaid_format() {
    let (stdout, success) = arch_gen(&[
        "/generate",
        "マイクロサービスを設計する",
        "--no-code",
        "-n",
        "1",
        "-f",
        "mermaid",
    ]);
    assert!(success);
    assert!(
        stdout.contains("graph TD"),
        "mermaid output should contain 'graph TD'\n{stdout}"
    );
}

#[test]
fn test_generate_markdown_format() {
    let (stdout, success) = arch_gen(&[
        "/generate",
        "イベント駆動システムを設計する",
        "--no-code",
        "-n",
        "1",
        "-f",
        "markdown",
    ]);
    assert!(success);
    assert!(stdout.contains("# Architecture Generation Report"));
}

#[test]
fn test_generate_plantuml_format() {
    let (stdout, success) = arch_gen(&[
        "/generate",
        "シンプルなWebアプリを設計する",
        "--no-code",
        "-n",
        "1",
        "-f",
        "plantuml",
    ]);
    assert!(success);
    assert!(
        stdout.contains("@startuml"),
        "plantuml output should contain '@startuml'\n{stdout}"
    );
    assert!(stdout.contains("@enduml"));
}

// ─── evaluate / export コマンド ───────────────────────────────────────────────

#[test]
fn test_evaluate_and_export_from_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let out_dir = tmp.path().to_str().unwrap();

    // まず generate で design.json を生成
    let (_, ok) = arch_gen(&[
        "/generate",
        "ECサイトを設計する",
        "--no-code",
        "-n",
        "1",
        "-o",
        out_dir,
    ]);
    assert!(ok, "generate should succeed");

    let design_path = tmp.path().join("design.json");
    assert!(design_path.exists(), "design.json should be written");

    // evaluate
    let (eval_out, eval_ok) = arch_gen(&["/evaluate", design_path.to_str().unwrap()]);
    assert!(eval_ok, "evaluate should succeed\n{eval_out}");
    assert!(eval_out.contains("Candidate"));

    // export mermaid
    let (merm_out, merm_ok) =
        arch_gen(&["/export", design_path.to_str().unwrap(), "-f", "mermaid"]);
    assert!(merm_ok, "export mermaid should succeed\n{merm_out}");
    assert!(merm_out.contains("graph TD"));

    // export markdown to file
    let md_path = tmp.path().join("report.md");
    let (_, md_ok) = arch_gen(&[
        "/export",
        design_path.to_str().unwrap(),
        "-f",
        "markdown",
        "-o",
        md_path.to_str().unwrap(),
    ]);
    assert!(md_ok, "export markdown to file should succeed");
    assert!(md_path.exists());
}

// ─── explain コマンド ─────────────────────────────────────────────────────────

#[test]
fn test_explain_from_generated_file() {
    let tmp = tempfile::TempDir::new().unwrap();
    let out_dir = tmp.path().to_str().unwrap();

    let (_, ok) = arch_gen(&[
        "/generate",
        "マイクロサービスAPIを設計する",
        "--no-code",
        "-n",
        "1",
        "-o",
        out_dir,
    ]);
    assert!(ok);

    let design_path = tmp.path().join("design.json");
    let (exp_out, exp_ok) = arch_gen(&["/explain", design_path.to_str().unwrap()]);
    assert!(exp_ok, "explain should succeed\n{exp_out}");
    assert!(exp_out.contains("Architecture Explanation"));
    assert!(exp_out.contains("Quality Analysis"));
}

// ─── refine コマンド ──────────────────────────────────────────────────────────

#[test]
fn test_refine_produces_refined_json() {
    let tmp = tempfile::TempDir::new().unwrap();
    let out_dir = tmp.path().to_str().unwrap();

    let (_, ok) = arch_gen(&[
        "/generate",
        "ECサイトを設計する",
        "--no-code",
        "-n",
        "1",
        "-o",
        out_dir,
    ]);
    assert!(ok);

    let design_path = tmp.path().join("design.json");
    let (_, ref_ok) = arch_gen(&[
        "/refine",
        design_path.to_str().unwrap(),
        "OAuth2認証とレート制限を追加してください",
    ]);
    assert!(ref_ok, "refine should succeed");

    let refined_path = tmp.path().join("design_refined.json");
    assert!(
        refined_path.exists(),
        "design_refined.json should be created"
    );
}

#[test]
fn test_refine_changes_output() {
    let tmp = tempfile::TempDir::new().unwrap();
    let out_dir = tmp.path().to_str().unwrap();

    let (_, ok) = arch_gen(&[
        "/generate",
        "シンプルなAPIを設計する",
        "--no-code",
        "-n",
        "1",
        "-o",
        out_dir,
    ]);
    assert!(ok);

    let design_path = tmp.path().join("design.json");

    // refine
    let (_, ref_ok) = arch_gen(&[
        "/refine",
        design_path.to_str().unwrap(),
        "キャッシュ層を追加してください",
    ]);
    assert!(ref_ok);

    let refined_path = tmp.path().join("design_refined.json");
    let original_json = std::fs::read_to_string(&design_path).unwrap();
    let refined_json = std::fs::read_to_string(&refined_path).unwrap();

    // refined の input には追加要件が含まれているはず
    let refined: serde_json::Value = serde_json::from_str(&refined_json).unwrap();
    assert!(
        refined["input"]
            .as_str()
            .unwrap_or("")
            .contains("キャッシュ層を追加してください"),
        "refined input should contain the additional requirement"
    );
    // 少なくとも input フィールドが変わっていることを確認
    let original: serde_json::Value = serde_json::from_str(&original_json).unwrap();
    assert_ne!(
        original["input"], refined["input"],
        "refined input should differ from original"
    );
}

// ─── エラーハンドリング ────────────────────────────────────────────────────────

#[test]
fn test_missing_design_file_returns_error() {
    let (_, success) = arch_gen(&["/evaluate", "/nonexistent/design.json"]);
    assert!(!success, "should exit non-zero for missing file");
}

#[test]
fn test_generate_file_reference() {
    // @path 形式でサンプル要件ファイルを読み込める
    let req_path = repo_root().join("examples/requirements/simple_webapp.txt");
    if req_path.exists() {
        let arg = format!("@{}", req_path.display());
        let (stdout, success) = arch_gen(&["/generate", &arg, "--no-code", "-n", "1"]);
        assert!(success, "generate with @file should succeed\n{stdout}");
    }
}
