use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use design_cli::coding::{
    apply_bootstrap_safety_policy, generate_code_change_set_with_target,
    prune_patches_for_target,
};
use integration_layer::{CodePatch, PatchOperation, RefactorPlanAction};

fn temp_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root =
        std::env::temp_dir().join(format!("design_cli_canonical_stream_{name}_{unique}"));
    fs::create_dir_all(root.join("src")).expect("create src dir");
    root
}

fn write_project(root: &Path) {
    fs::create_dir_all(root.join("src/nl")).expect("create nl dir");
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"canonical_stream\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write Cargo.toml");
    fs::write(root.join("src/main.rs"), "mod nl;\nfn main() {}\n").expect("write main");
    fs::write(root.join("src/nl/mod.rs"), "pub mod goal;\n").expect("write nl mod");
    fs::write(root.join("src/nl/goal.rs"), "pub fn resolve() {}\n").expect("write goal");
    fs::write(root.join("src/coding.rs"), "pub fn run() {}\n").expect("write coding");
}

fn unrelated_patches() -> Vec<CodePatch> {
    vec![
        CodePatch {
            patch_id: "unrelated_1".to_string(),
            action: RefactorPlanAction::IntroduceInterface {
                between: ("adapter".to_string(), "controller".to_string()),
            },
            operations: vec![PatchOperation::CreateInterface {
                name: "AdapterControllerInterface".to_string(),
                between: ("adapter".to_string(), "controller".to_string()),
            }],
            description: "introduce interface between adapter and controller".to_string(),
        },
        CodePatch {
            patch_id: "unrelated_2".to_string(),
            action: RefactorPlanAction::MoveDependency {
                from: "determinism".to_string(),
                to: "engine".to_string(),
                via: None,
            },
            operations: vec![PatchOperation::UpdateDependency {
                from: "determinism".to_string(),
                to: "engine".to_string(),
                via: None,
            }],
            description: "move determinism -> engine".to_string(),
        },
        CodePatch {
            patch_id: "unrelated_3".to_string(),
            action: RefactorPlanAction::MoveDependency {
                from: "dependency".to_string(),
                to: "runtime".to_string(),
                via: None,
            },
            operations: vec![PatchOperation::UpdateDependency {
                from: "dependency".to_string(),
                to: "runtime".to_string(),
                via: None,
            }],
            description: "move dependency -> runtime".to_string(),
        },
    ]
}

// ─── Case 1: goal.rs — unrelated patches produce 0 changes ───────────────────

#[test]
fn goal_rs_target_unrelated_patches_produce_zero_changes() {
    let root = temp_workspace("goal_unrelated");
    write_project(&root);

    let target = Path::new("src/nl/goal.rs");
    let patches = unrelated_patches();

    let change_set = generate_code_change_set_with_target(&root, &patches, Some(target))
        .expect("generate should succeed");

    assert!(
        change_set.changes.is_empty(),
        "unrelated patches must produce 0 changes for goal.rs, got: {:?}",
        change_set.changes.iter().map(|c| &c.file_path).collect::<Vec<_>>()
    );
    assert_eq!(
        change_set.summary.total_changes, 0,
        "summary.total_changes must be 0"
    );
}

// ─── Case 2: diff stream consistency — pruned count matches change count ──────

#[test]
fn pruned_patches_and_changes_are_consistent() {
    let root = temp_workspace("diff_consistency");
    write_project(&root);

    let target = Path::new("src/nl/goal.rs");
    let raw_patches = unrelated_patches();

    // Canonical stream: bootstrap policy → semantic prune
    let after_bootstrap = apply_bootstrap_safety_policy(&raw_patches, Some(target));
    let canonical = prune_patches_for_target(&after_bootstrap, target);

    let change_set = generate_code_change_set_with_target(&root, &raw_patches, Some(target))
        .expect("generate should succeed");

    // When canonical is empty, changes must also be empty
    if canonical.is_empty() {
        assert!(
            change_set.changes.is_empty(),
            "canonical patches empty but changes non-empty: {:?}",
            change_set.changes.iter().map(|c| &c.file_path).collect::<Vec<_>>()
        );
    }

    // changes count can only come from canonical patches (never from pruned-out patches)
    // Verify no adapter/controller/determinism/engine/dependency files were touched
    for change in &change_set.changes {
        let path = &change.file_path;
        assert!(
            !path.contains("adapter")
                && !path.contains("controller")
                && !path.contains("determinism")
                && !path.contains("engine")
                && !path.contains("dependency"),
            "change from pruned-out patch leaked into output: {path}"
        );
    }
}

// ─── Case 3: self-host coding.rs — unrelated patches produce 0 changes ───────

#[test]
fn coding_rs_self_host_unrelated_patches_produce_zero_changes() {
    let root = temp_workspace("coding_self_host");
    write_project(&root);

    // Patches that don't touch coding/app/source_index cluster
    let patches = vec![
        CodePatch {
            patch_id: "foreign_1".to_string(),
            action: RefactorPlanAction::IntroduceInterface {
                between: ("adapter".to_string(), "controller".to_string()),
            },
            operations: vec![PatchOperation::CreateInterface {
                name: "AdapterControllerIface".to_string(),
                between: ("adapter".to_string(), "controller".to_string()),
            }],
            description: "introduce interface adapter-controller".to_string(),
        },
        CodePatch {
            patch_id: "foreign_2".to_string(),
            action: RefactorPlanAction::MoveDependency {
                from: "agent".to_string(),
                to: "capability".to_string(),
                via: None,
            },
            operations: vec![PatchOperation::UpdateDependency {
                from: "agent".to_string(),
                to: "capability".to_string(),
                via: None,
            }],
            description: "agent -> capability migration".to_string(),
        },
    ];

    let target = Path::new("src/coding.rs");
    let change_set = generate_code_change_set_with_target(&root, &patches, Some(target))
        .expect("generate should succeed");

    assert!(
        change_set.changes.is_empty(),
        "unrelated patches must produce 0 changes for coding.rs self-host, got: {:?}",
        change_set.changes.iter().map(|c| &c.file_path).collect::<Vec<_>>()
    );
    assert_eq!(
        change_set.summary.total_changes, 0,
        "breaking_count proxy: total_changes must be 0"
    );
}

// ─── canonical stream is single-binding — no dual patch stream ───────────────

#[test]
fn canonical_stream_is_sole_source_for_downstream() {
    let target = Path::new("src/nl/goal.rs");
    let raw = unrelated_patches();

    // Simulate the canonical pipeline
    let after_bootstrap = apply_bootstrap_safety_policy(&raw, Some(target));
    let canonical = prune_patches_for_target(&after_bootstrap, target);

    // The canonical stream must be the only authoritative set
    // Verify it differs from raw (pruning actually happened)
    assert!(
        canonical.len() < raw.len() || canonical.is_empty(),
        "canonical stream should be smaller than raw for goal.rs with unrelated patches"
    );

    // No id from canonical should be among the unrelated patch ids
    let unrelated_ids = ["unrelated_1", "unrelated_2", "unrelated_3"];
    for patch in &canonical {
        assert!(
            !unrelated_ids.contains(&patch.patch_id.as_str()),
            "unrelated patch '{}' survived into canonical stream",
            patch.patch_id
        );
    }
}
