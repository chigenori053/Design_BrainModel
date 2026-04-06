use design_cli::coding::{
    ChangeSummary, ChangeType, CodeChange, CodeChangeSet, CodingOptions, DiffHunk,
    collect_apply_target_resolutions, execute_code_change_set, generate_code_change_set,
    resolve_apply_paths_for_patches,
};
use design_cli::refactor::PatchScope;
use integration_layer::{CodePatch, PatchOperation, RefactorPlanAction};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_coding_mutation_flow_{name}_{unique}"));
    fs::create_dir_all(dir.join("src")).expect("src");
    dir
}

fn write_rust_project(root: &Path, main_body: &str) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"coding_mutation_flow\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(root.join("src/main.rs"), main_body).expect("main");
}

fn move_dependency_patch(from: &str, to: &str, via: Option<&str>) -> CodePatch {
    CodePatch {
        patch_id: "p1".to_string(),
        action: RefactorPlanAction::MoveDependency {
            from: from.to_string(),
            to: to.to_string(),
            via: via.map(str::to_string),
        },
        operations: vec![PatchOperation::UpdateDependency {
            from: from.to_string(),
            to: to.to_string(),
            via: via.map(str::to_string),
        }],
        description: "move dependency".to_string(),
        target_file: Default::default(),
    }
}

fn write_candidate_snapshot(
    workspace: &Path,
    candidate_id: &str,
    crate_name: &str,
    logical_name: &str,
    source_path: &str,
) {
    fs::create_dir_all(workspace.join(".dbm/refactor/candidates")).expect("candidate dir");
    let mut hasher = Sha256::new();
    hasher.update(fs::read(workspace.join(source_path)).expect("read source"));
    let file_hash = format!("{:x}", hasher.finalize());
    fs::write(
        workspace.join(format!(".dbm/refactor/candidates/{candidate_id}.json")),
        format!(
            r#"{{
  "candidate_id": "{candidate_id}",
  "module_id": {{ "crate_name": "{crate_name}", "module_path": "{logical_name}" }},
  "logical_name": "{logical_name}",
  "kind": "RemoveDependency",
  "operation": "RemoveDependency",
  "title": "candidate",
  "rationale": "integration",
  "confidence_milli": 900,
  "confidence": 0.9,
  "from_node": {{
    "qualified_id": {{ "crate_name": "{crate_name}", "module_path": "{logical_name}" }},
    "logical_name": "{logical_name}",
    "source_path": "{source_path}"
  }},
  "to_node": {{
    "qualified_id": {{ "crate_name": "{crate_name}", "module_path": "{logical_name}" }},
    "logical_name": "{logical_name}",
    "source_path": "{source_path}"
  }},
  "patch_plan": {{ "RemoveDependency": {{ "from": "{logical_name}", "to": "world" }} }},
  "source_path": "{source_path}",
  "preview_hash": "sha256:{file_hash}",
  "base_file_hash": "{file_hash}",
  "target_nodes": ["{logical_name}"],
  "target_edges": [],
  "target": {{ "RemoveDependency": {{ "from": "{logical_name}", "to": "world" }} }}
}}"#
        ),
    )
    .expect("candidate");
}

#[test]
fn diff_generation_moves_to_integration() {
    let root = temp_workspace("diff_generation");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"coding_diff_generation\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        root.join("src/lib.rs"),
        "pub mod renderer;\npub mod renderer_world_interface;\npub mod world;\n",
    )
    .expect("lib");
    fs::write(
        root.join("src/renderer.rs"),
        "use crate::world;\nfn render() {}\n",
    )
    .expect("renderer");
    fs::write(root.join("src/world.rs"), "pub fn world() {}\n").expect("world");
    fs::write(
        root.join("src/renderer_world_interface.rs"),
        "pub trait RendererWorldInterface {}\n",
    )
    .expect("interface");

    let change_set = generate_code_change_set(
        &root,
        &[move_dependency_patch(
            "renderer",
            "world",
            Some("renderer_world_interface"),
        )],
    )
    .expect("change set");

    let change = change_set.changes.first().expect("change");
    assert_eq!(change.change_type, ChangeType::ModifyFile);
    assert_eq!(change.file_path, "src/renderer.rs");
    assert!(!change.hunks[0].replacement.contains("use crate::world;"));
}

#[test]
fn preview_apply_deterministic_path_resolution_uses_snapshot_in_integration() {
    let root = temp_workspace("deterministic_path_resolution");
    fs::create_dir_all(root.join("src/runtime")).expect("runtime");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"coding_test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        root.join("src/main.rs"),
        "mod determinism_world_interface;\nmod runtime;\nmod world;\nfn main() {}\n",
    )
    .expect("main");
    fs::write(root.join("src/runtime/mod.rs"), "pub mod determinism;\n").expect("mod");
    fs::write(root.join("src/world.rs"), "pub fn noop() {}\n").expect("world");
    fs::write(
        root.join("src/determinism_world_interface.rs"),
        "pub trait DeterminismWorldInterface {}\n",
    )
    .expect("interface");
    fs::write(
        root.join("src/runtime/determinism.rs"),
        "use crate::world;\npub fn check() {}\n",
    )
    .expect("determinism");
    write_candidate_snapshot(
        &root,
        "determinism",
        "coding_test",
        "determinism",
        "src/runtime/determinism.rs",
    );

    let patches = vec![move_dependency_patch(
        "determinism",
        "world",
        Some("determinism_world_interface"),
    )];
    let resolved = resolve_apply_paths_for_patches(&root, &patches, Some("determinism"))
        .expect("resolved paths");
    assert_eq!(
        resolved.get("determinism"),
        Some(&PathBuf::from("src/runtime/determinism.rs"))
    );

    let resolutions = collect_apply_target_resolutions(&root, &patches, None, &resolved)
        .expect("apply target resolutions");
    assert_eq!(resolutions.len(), 1);
    assert_eq!(
        resolutions[0].resolved_relative_path,
        PathBuf::from("src/runtime/determinism.rs")
    );
    assert_eq!(resolutions[0].resolution_strategy, "candidate_snapshot");
}

#[test]
fn apply_success_moves_to_integration() {
    let root = temp_workspace("apply_success");
    write_rust_project(&root, "fn main() {}\n");
    let change_set = CodeChangeSet {
        patches: vec![],
        changes: vec![CodeChange {
            file_path: "src/world_service.rs".to_string(),
            change_type: ChangeType::CreateFile,
            hunks: vec![DiffHunk {
                start_line: 1,
                end_line: 1,
                replacement: "pub struct WorldService {}\n".to_string(),
            }],
        }],
        summary: ChangeSummary {
            total_changes: 1,
            create_files: 1,
            modify_files: 0,
            move_files: 0,
        },
        canonical_target: None,
    };

    let result = execute_code_change_set(
        &root,
        &change_set,
        &CodingOptions {
            apply: true,
            check: false,
            no_build: false,
            backup: false,
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
            patch_scope: PatchScope::WorkspaceWide,
            explicit_target: None,
        },
        None,
    )
    .expect("apply");

    assert_eq!(result.status, "applied");
    assert!(result.build_ok);
    assert!(result.transactional_apply.is_some());
    assert!(root.join("src/world_service.rs").exists());
}
