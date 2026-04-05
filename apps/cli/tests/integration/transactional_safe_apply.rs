use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use design_cli::refactor::{
    SandboxWritePreview, TransactionExecutionPreview, execute_transactional_safe_apply,
};

fn current_dir_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn temp_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("design_cli_tx_apply_{name}_{unique}"));
    fs::create_dir_all(root.join("crates/runtime/runtime_vm/src")).expect("workspace dirs");
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/runtime/runtime_vm\"]\nresolver = \"2\"\n",
    )
    .expect("workspace cargo");
    fs::write(
        root.join("crates/runtime/runtime_vm/Cargo.toml"),
        "[package]\nname = \"runtime_vm\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("crate cargo");
    fs::write(
        root.join("crates/runtime/runtime_vm/src/lib.rs"),
        "pub mod adapter;\n",
    )
    .expect("lib");
    fs::write(
        root.join("crates/runtime/runtime_vm/src/adapter.rs"),
        "pub fn adapt() -> &'static str {\n    \"ok\"\n}\n",
    )
    .expect("adapter");
    root
}

fn execution_preview(candidate_id: &str) -> TransactionExecutionPreview {
    TransactionExecutionPreview {
        candidate_id: candidate_id.to_string(),
        allowed: true,
        executed: false,
        sandbox_write: SandboxWritePreview {
            enabled: true,
            target_files: vec!["crates/runtime/runtime_vm/src/adapter.rs".to_string()],
        },
        steps: vec![
            "sandbox patch write".to_string(),
            "cargo check -p runtime_vm".to_string(),
            "commit preview".to_string(),
            "rollback on fail".to_string(),
        ],
        rollback_guaranteed: true,
        write: false,
    }
}

fn with_workspace_root<T>(root: &Path, f: impl FnOnce() -> T) -> T {
    let _guard = current_dir_lock().lock().expect("lock");
    let previous = std::env::current_dir().expect("cwd");
    std::env::set_current_dir(root).expect("set cwd");
    let result = f();
    std::env::set_current_dir(previous).expect("restore cwd");
    result
}

#[test]
fn transactional_safe_apply_succeeds_in_sandbox() {
    let root = temp_workspace("success");
    let result = with_workspace_root(&root, || {
        execute_transactional_safe_apply(&execution_preview("cut-adapter-world"))
            .expect("transaction result")
    });

    assert!(result.success);
    assert!(result.executed);
    assert!(!result.rollback_executed);
    assert!(root.join(&result.sandbox_root).exists());

    let original = fs::read_to_string(root.join("crates/runtime/runtime_vm/src/adapter.rs"))
        .expect("original adapter");
    assert!(!original.contains("DBM Preview Apply"));
}

#[test]
fn transactional_safe_apply_rolls_back_on_cargo_check_fail() {
    let root = temp_workspace("failure");
    let result = with_workspace_root(&root, || {
        execute_transactional_safe_apply(&execution_preview("invalid-transaction"))
            .expect("transaction result")
    });

    assert!(!result.success);
    assert!(result.rollback_executed);
    assert!(!root.join(&result.sandbox_root).exists());
}

#[test]
fn transactional_safe_apply_creates_sandbox_and_writes_target_file() {
    let root = temp_workspace("sandbox");
    let result = with_workspace_root(&root, || {
        execute_transactional_safe_apply(&execution_preview("cut-adapter-world"))
            .expect("transaction result")
    });

    assert!(Path::new(&root.join(&result.sandbox_root)).exists());
    let sandbox_file = root
        .join(&result.sandbox_root)
        .join("crates/runtime/runtime_vm/src/adapter.rs");
    let sandbox_contents = fs::read_to_string(sandbox_file).expect("sandbox adapter");
    assert!(sandbox_contents.contains("DBM Preview Apply"));
}
