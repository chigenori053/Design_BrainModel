use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use design_cli::refactor::{PromoteResult, TransactionResult, promote_sandbox_to_workspace};

use design_cli::test_support::with_current_dir;

fn temp_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("design_cli_promote_{name}_{unique}"));
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
        "pub fn adapt() -> &'static str {\n    \"workspace\"\n}\n",
    )
    .expect("adapter");
    root
}

fn write_sandbox_file(root: &Path, relative_root: &str, adapter_contents: &str) {
    let sandbox_root = root.join(relative_root);
    fs::create_dir_all(sandbox_root.join("crates/runtime/runtime_vm/src")).expect("sandbox dirs");
    fs::write(
        sandbox_root.join("crates/runtime/runtime_vm/src/adapter.rs"),
        adapter_contents,
    )
    .expect("sandbox adapter");
}

fn tx_result(root_name: &str) -> TransactionResult {
    TransactionResult {
        executed: true,
        success: true,
        sandbox_root: format!(".dbm/sandbox/{root_name}"),
        written_files: vec!["crates/runtime/runtime_vm/src/adapter.rs".to_string()],
        cargo_check: "passed".to_string(),
        rollback_executed: false,
    }
}

fn with_workspace_root<T>(root: &Path, f: impl FnOnce() -> T) -> T {
    with_current_dir(root, f)
}

#[test]
fn promote_workspace_patch_succeeds_when_confirmed() {
    let root = temp_workspace("success");
    write_sandbox_file(
        &root,
        ".dbm/sandbox/tx-success",
        "pub fn adapt() -> &'static str {\n    \"sandbox\"\n}\n",
    );

    let result = with_workspace_root(&root, || {
        promote_sandbox_to_workspace(&tx_result("tx-success"), true).expect("promote result")
    });

    assert!(result.workspace_write);
    assert!(!result.rollback_executed);
    assert_eq!(
        fs::read_to_string(root.join("crates/runtime/runtime_vm/src/adapter.rs"))
            .expect("workspace adapter"),
        "pub fn adapt() -> &'static str {\n    \"sandbox\"\n}\n"
    );
}

#[test]
fn promote_workspace_patch_requires_confirmation() {
    let root = temp_workspace("unconfirmed");
    write_sandbox_file(
        &root,
        ".dbm/sandbox/tx-unconfirmed",
        "pub fn adapt() -> &'static str {\n    \"sandbox\"\n}\n",
    );

    let result = with_workspace_root(&root, || {
        promote_sandbox_to_workspace(&tx_result("tx-unconfirmed"), false)
    });

    assert!(result.is_none());
}

#[test]
fn promote_workspace_patch_rolls_back_on_cargo_check_fail() {
    let root = temp_workspace("fail");
    write_sandbox_file(
        &root,
        ".dbm/sandbox/tx-fail",
        "pub fn adapt() -> &'static str {\n    \"sandbox\"\n}\nthis is not valid rust\n",
    );

    let result = with_workspace_root(&root, || {
        promote_sandbox_to_workspace(&tx_result("tx-fail"), true).expect("promote result")
    });

    assert_eq!(
        result,
        PromoteResult {
            confirmed: true,
            workspace_write: true,
            written_files: vec!["crates/runtime/runtime_vm/src/adapter.rs".to_string()],
            cargo_check: "failed".to_string(),
            rollback_executed: true,
        }
    );
    assert_eq!(
        fs::read_to_string(root.join("crates/runtime/runtime_vm/src/adapter.rs"))
            .expect("workspace adapter"),
        "pub fn adapt() -> &'static str {\n    \"workspace\"\n}\n"
    );
}
