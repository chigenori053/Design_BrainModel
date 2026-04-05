use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use design_cli::coding::{
    ChangeSummary, ChangeType, CodeChange, CodeChangeSet, CodingOptions, DiffHunk,
    apply_bootstrap_safety_policy, execute_code_change_set, generate_code_change_set_with_target,
};
use design_cli::refactor::PatchScope;
use integration_layer::{CodePatch, PatchOperation, RefactorPlanAction};

fn temp_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("design_cli_target_scope_{name}_{unique}"));
    fs::create_dir_all(root.join("src/runtime")).expect("create runtime dir");
    root
}

fn write_rust_project(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"coding_target_scope\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write cargo");
    fs::write(root.join("src/main.rs"), "mod runtime;\nfn main() {}\n").expect("write main");
    fs::write(root.join("src/runtime/mod.rs"), "pub fn noop() {}\n").expect("write mod");
    fs::write(
        root.join("src/runtime/repl.rs"),
        "use crate::world;\npub fn run() {}\n",
    )
    .expect("write target");
}

fn target_scope_options(target: &str) -> CodingOptions {
    CodingOptions {
        apply: true,
        check: true,
        no_build: true,
        backup: true,
        format: false,
        safe_mode: true,
        auto_commit: false,
        confirm_commit: false,
        prompt_commit: false,
        auto_push: false,
        confirm_push: false,
        auto_pr: false,
        confirm_pr: false,
        pr_base: "main".to_string(),
        patch_scope: PatchScope::ExplicitTargetOnly,
        explicit_target: Some(PathBuf::from(target)),
    }
}

#[test]
fn explicit_target_prunes_runtime_vm_candidate() {
    let root = temp_workspace("runtime_vm_prune");
    write_rust_project(&root);
    let patches = vec![CodePatch {
        patch_id: "p1".to_string(),
        action: RefactorPlanAction::IntroduceInterface {
            between: ("runtime::repl".to_string(), "world".to_string()),
        },
        operations: vec![PatchOperation::CreateInterface {
            name: "ReplWorldInterface".to_string(),
            between: ("runtime::repl".to_string(), "world".to_string()),
        }],
        description: "introduce interface".to_string(),
        target_file: Default::default(),
    }];

    let change_set = generate_code_change_set_with_target(
        &root,
        &patches,
        Some(Path::new("src/runtime/repl.rs")),
    )
    .expect("non-target candidate should be pruned before patch generation");
    assert!(change_set.changes.is_empty(), "{change_set:?}");
}

#[test]
fn target_mode_blocks_mod_registration() {
    let root = temp_workspace("mod_registration");
    write_rust_project(&root);
    let change_set = CodeChangeSet {
        patches: vec![],
        changes: vec![
            CodeChange {
                file_path: "src/runtime/repl.rs".to_string(),
                change_type: ChangeType::ModifyFile,
                hunks: vec![DiffHunk {
                    start_line: 1,
                    end_line: 2,
                    replacement: "use crate::repl_world_interface;\npub fn run() {}\n".to_string(),
                }],
            },
            CodeChange {
                file_path: "src/runtime/mod.rs".to_string(),
                change_type: ChangeType::ModifyFile,
                hunks: vec![DiffHunk {
                    start_line: 1,
                    end_line: 1,
                    replacement: "pub mod repl;\npub mod repl_world_interface;\n".to_string(),
                }],
            },
        ],
        summary: ChangeSummary {
            total_changes: 2,
            create_files: 0,
            modify_files: 2,
            move_files: 0,
        },
    };

    let result = execute_code_change_set(
        &root,
        &change_set,
        &target_scope_options("src/runtime/repl.rs"),
        None,
    )
    .expect("scope violation returns failed execution");
    assert_eq!(result.status, "failed");
    assert!(result.rolled_back);
    assert_eq!(
        result.reason.as_deref(),
        Some(
            "patch_scope_violation: target file src/runtime/repl.rs != generated patch path src/runtime/mod.rs"
        )
    );
}

#[test]
fn explicit_target_drops_cross_crate_candidates() {
    let root = temp_workspace("cross_crate_drop");
    write_rust_project(&root);
    let patches = vec![CodePatch {
        patch_id: "p1".to_string(),
        action: RefactorPlanAction::SplitModule {
            target: "runtime::repl".to_string(),
        },
        operations: vec![PatchOperation::SplitModule {
            module: "runtime::repl".to_string(),
            new_modules: vec![
                "adapter_app_interface".to_string(),
                "runtime_vm".to_string(),
            ],
        }],
        description: "drop cross crate candidates".to_string(),
        target_file: Default::default(),
    }];

    let change_set = generate_code_change_set_with_target(
        &root,
        &patches,
        Some(Path::new("src/runtime/repl.rs")),
    )
    .expect("cross-crate candidates should be dropped");
    assert!(change_set.changes.is_empty(), "{change_set:?}");
}

#[test]
fn patch_scope_violation_no_longer_occurs_for_same_file() {
    let root = temp_workspace("same_file_import_fix");
    write_rust_project(&root);
    let patches = vec![CodePatch {
        patch_id: "p1".to_string(),
        action: RefactorPlanAction::MoveDependency {
            from: "runtime::repl".to_string(),
            to: "world".to_string(),
            via: Some("repl_world_interface".to_string()),
        },
        operations: vec![PatchOperation::UpdateDependency {
            from: "runtime::repl".to_string(),
            to: "world".to_string(),
            via: Some("repl_world_interface".to_string()),
        }],
        description: "import fix".to_string(),
        target_file: Default::default(),
    }];

    let change_set = generate_code_change_set_with_target(
        &root,
        &patches,
        Some(Path::new("src/runtime/repl.rs")),
    )
    .expect("same-file edit should be allowed");

    assert_eq!(change_set.changes.len(), 1);
    assert_eq!(change_set.changes[0].file_path, "src/runtime/repl.rs");
    assert_eq!(change_set.changes[0].change_type, ChangeType::ModifyFile);
}

#[test]
fn rollback_on_scope_violation() {
    let root = temp_workspace("rollback");
    write_rust_project(&root);
    let original = fs::read_to_string(root.join("src/runtime/repl.rs")).expect("read original");
    let change_set = CodeChangeSet {
        patches: vec![],
        changes: vec![
            CodeChange {
                file_path: "src/runtime/repl.rs".to_string(),
                change_type: ChangeType::ModifyFile,
                hunks: vec![DiffHunk {
                    start_line: 1,
                    end_line: 2,
                    replacement: "use crate::repl_world_interface;\npub fn run() {}\n".to_string(),
                }],
            },
            CodeChange {
                file_path: "src/repl_world_interface.rs".to_string(),
                change_type: ChangeType::CreateFile,
                hunks: vec![DiffHunk {
                    start_line: 1,
                    end_line: 1,
                    replacement: "pub trait ReplWorldInterface {}\n".to_string(),
                }],
            },
        ],
        summary: ChangeSummary {
            total_changes: 2,
            create_files: 1,
            modify_files: 1,
            move_files: 0,
        },
    };

    let result = execute_code_change_set(
        &root,
        &change_set,
        &target_scope_options("src/runtime/repl.rs"),
        None,
    )
    .expect("scope violation returns failed execution");

    assert_eq!(result.status, "failed");
    assert!(result.rolled_back);
    assert_eq!(
        result.reason.as_deref(),
        Some(
            "patch_scope_violation: target file src/runtime/repl.rs != generated patch path src/repl_world_interface.rs"
        )
    );
    assert_eq!(
        fs::read_to_string(root.join("src/runtime/repl.rs")).expect("read target"),
        original
    );
    assert!(!root.join("src/repl_world_interface.rs").exists());
}

#[test]
fn source_index_bootstrap_blocks_cross_module_changes() {
    for patches in [
        vec![CodePatch {
            patch_id: "p1".to_string(),
            action: RefactorPlanAction::IntroduceInterface {
                between: ("source_index".to_string(), "world".to_string()),
            },
            operations: vec![PatchOperation::CreateInterface {
                name: "AgentDomainInterface".to_string(),
                between: ("source_index".to_string(), "world".to_string()),
            }],
            description: "introduce interface".to_string(),
            target_file: Default::default(),
        }],
        vec![CodePatch {
            patch_id: "p1".to_string(),
            action: RefactorPlanAction::MoveDependency {
                from: "source_index".to_string(),
                to: "world".to_string(),
                via: Some("adapter_app_interface".to_string()),
            },
            operations: vec![PatchOperation::UpdateDependency {
                from: "source_index".to_string(),
                to: "world".to_string(),
                via: Some("adapter_app_interface".to_string()),
            }],
            description: "move dependency".to_string(),
            target_file: Default::default(),
        }],
    ] {
        let filtered = apply_bootstrap_safety_policy(
            &patches,
            Some(Path::new("apps/cli/src/source_index.rs")),
        );
        assert!(filtered.is_empty(), "{filtered:?}");
    }
}

#[test]
fn source_index_bootstrap_disables_import_rebinding() {
    let root = temp_workspace("source_index_import_rebinding");
    fs::create_dir_all(root.join("apps/cli/src")).expect("create source_index dir");
    fs::write(
        root.join("apps/cli/src/source_index.rs"),
        "use crate::world;\npub fn resolve() {}\n",
    )
    .expect("write source_index");
    let patches = vec![CodePatch {
        patch_id: "p1".to_string(),
        action: RefactorPlanAction::MoveDependency {
            from: "source_index".to_string(),
            to: "world".to_string(),
            via: Some("adapter_app_interface".to_string()),
        },
        operations: vec![PatchOperation::UpdateDependency {
            from: "source_index".to_string(),
            to: "world".to_string(),
            via: Some("adapter_app_interface".to_string()),
        }],
        description: "import rebinding".to_string(),
        target_file: Default::default(),
    }];

    let filtered =
        apply_bootstrap_safety_policy(&patches, Some(Path::new("apps/cli/src/source_index.rs")));
    let change_set = generate_code_change_set_with_target(
        &root,
        &filtered,
        Some(Path::new("apps/cli/src/source_index.rs")),
    )
    .expect("bootstrap filtered change set");

    assert!(change_set.changes.is_empty(), "{change_set:?}");
}

#[test]
fn source_index_bootstrap_same_file_local_edit_only() {
    let root = temp_workspace("source_index_local_edit");
    fs::create_dir_all(root.join("apps/cli/src")).expect("create source_index dir");
    let target = root.join("apps/cli/src/source_index.rs");
    let original = "pub fn resolve() {}\n";
    fs::write(&target, original).expect("write source_index");

    let change_set = CodeChangeSet {
        patches: vec![],
        changes: vec![CodeChange {
            file_path: "apps/cli/src/source_index.rs".to_string(),
            change_type: ChangeType::ModifyFile,
            hunks: vec![DiffHunk {
                start_line: 1,
                end_line: 1,
                replacement: "pub fn resolve() {}\nfn helper() {}\n".to_string(),
            }],
        }],
        summary: ChangeSummary {
            total_changes: 1,
            create_files: 0,
            modify_files: 1,
            move_files: 0,
        },
    };

    let result = execute_code_change_set(
        &root,
        &change_set,
        &target_scope_options("apps/cli/src/source_index.rs"),
        None,
    )
    .expect("same-file local edit allowed");

    assert_eq!(result.status, "applied");
    assert!(!result.rolled_back);
    assert_eq!(
        fs::read_to_string(target).expect("read source_index"),
        "pub fn resolve() {}\nfn helper() {}\n"
    );
}
