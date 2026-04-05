use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use design_cli::coding::{
    generate_code_change_set_with_target, patch_matches_cluster, prune_patches_for_target,
    semantic_cluster_for_target,
};
use integration_layer::{CodePatch, PatchOperation, RefactorPlanAction};

fn temp_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("design_cli_narrow_{name}_{unique}"));
    fs::create_dir_all(root.join("src/nl")).expect("create nl dir");
    root
}

fn write_project(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"narrow_test\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("write Cargo.toml");
    fs::write(
        root.join("src/main.rs"),
        "mod nl;\nmod app;\nfn main() {}\n",
    )
    .expect("write main");
    fs::write(root.join("src/nl/mod.rs"), "pub mod goal;\n").expect("write nl mod");
    fs::write(root.join("src/nl/goal.rs"), "pub fn resolve() {}\n").expect("write goal");
    fs::write(root.join("src/app.rs"), "pub fn run() {}\n").expect("write app");
}

fn adapter_app_patch() -> CodePatch {
    CodePatch {
        patch_id: "adapter_app".to_string(),
        action: RefactorPlanAction::IntroduceInterface {
            between: ("adapter".to_string(), "app".to_string()),
        },
        operations: vec![PatchOperation::CreateInterface {
            name: "adapter_app_interface".to_string(),
            between: ("adapter".to_string(), "app".to_string()),
        }],
        description: "introduce adapter-app boundary".to_string(),
        target_file: Default::default(),
    }
}

fn nl_goal_patch() -> CodePatch {
    CodePatch {
        patch_id: "nl_goal".to_string(),
        action: RefactorPlanAction::MoveDependency {
            from: "nl".to_string(),
            to: "goal".to_string(),
            via: None,
        },
        operations: vec![PatchOperation::UpdateDependency {
            from: "nl".to_string(),
            to: "goal".to_string(),
            via: None,
        }],
        description: "move nl -> goal".to_string(),
        target_file: Default::default(),
    }
}

// ─── R1: cluster does not contain "app" ──────────────────────────────────────

#[test]
fn semantic_clusters_keep_only_expected_tokens() {
    let cluster = semantic_cluster_for_target(Path::new("apps/cli/src/nl/goal.rs"));
    assert!(
        !cluster.contains(&"app"),
        "\"app\" must not be in nl cluster: {cluster:?}"
    );
    assert!(
        cluster.contains(&"nl"),
        "\"nl\" must be in cluster: {cluster:?}"
    );
    assert!(
        cluster.contains(&"goal"),
        "\"goal\" must be in cluster: {cluster:?}"
    );

    let cluster = semantic_cluster_for_target(Path::new("apps/cli/src/coding.rs"));
    assert!(
        !cluster.contains(&"app"),
        "\"app\" must not be in coding cluster: {cluster:?}"
    );
    assert!(
        cluster.contains(&"coding"),
        "\"coding\" must be in cluster: {cluster:?}"
    );
}

// ─── R2: exact module token — adapter_app_interface must not match nl cluster ─

#[test]
fn app_related_patches_do_not_match_nl_cluster() {
    for patch in [
        CodePatch {
            patch_id: "leaky".to_string(),
            action: RefactorPlanAction::IntroduceInterface {
                between: ("adapter".to_string(), "app".to_string()),
            },
            operations: vec![PatchOperation::CreateInterface {
                name: "adapter_app_interface".to_string(),
                between: ("adapter".to_string(), "app".to_string()),
            }],
            description: "adapter app interface".to_string(),
            target_file: Default::default(),
        },
        adapter_app_patch(),
    ] {
        assert!(
            !patch_matches_cluster(&patch, &["nl", "goal"]),
            "app-related patch must not leak through nl cluster: {patch:?}"
        );
    }
}

// ─── R2: exact "app" module IS allowed in app.rs cluster ─────────────────────

#[test]
fn app_rs_target_has_own_cluster() {
    // apps/cli/src/app.rs has no declared cluster → empty → pass-through
    // This verifies that app.rs does not accidentally get the nl cluster
    let nl_cluster = semantic_cluster_for_target(Path::new("apps/cli/src/nl/goal.rs"));
    let app_cluster = semantic_cluster_for_target(Path::new("apps/cli/src/app.rs"));
    assert_ne!(
        nl_cluster, app_cluster,
        "app.rs and nl/goal.rs must not share a cluster"
    );
}

// ─── R3: deep exact match — only module fields, not interface names ───────────

#[test]
fn interface_name_with_nl_substring_does_not_match() {
    // Interface name "nl_boundary_interface" contains "nl" but exact token is
    // "nl_boundary_interface" which != "nl" — must not match
    let patch = CodePatch {
        patch_id: "fake_nl".to_string(),
        action: RefactorPlanAction::IntroduceInterface {
            between: ("adapter".to_string(), "renderer".to_string()),
        },
        operations: vec![PatchOperation::CreateInterface {
            name: "nl_boundary_interface".to_string(),
            between: ("adapter".to_string(), "renderer".to_string()),
        }],
        description: "fake nl interface".to_string(),
        target_file: Default::default(),
    };
    assert!(
        !patch_matches_cluster(&patch, &["nl", "goal"]),
        "interface name must not be checked — only between modules"
    );
}

#[test]
fn namespaced_nl_module_matches_nl_cluster() {
    // "nl::goal" splits on "::" to ["nl", "goal"] — "nl" exactly matches
    let patch = CodePatch {
        patch_id: "nl_ns".to_string(),
        action: RefactorPlanAction::MoveDependency {
            from: "nl::goal".to_string(),
            to: "renderer".to_string(),
            via: None,
        },
        operations: vec![PatchOperation::UpdateDependency {
            from: "nl::goal".to_string(),
            to: "renderer".to_string(),
            via: None,
        }],
        description: "move nl::goal".to_string(),
        target_file: Default::default(),
    };
    assert!(
        patch_matches_cluster(&patch, &["nl", "goal"]),
        "namespaced nl::goal must match nl cluster"
    );
}

// ─── R4: description fallback is prohibited ───────────────────────────────────

#[test]
fn descriptions_do_not_grant_cluster_membership() {
    for (patch, cluster) in [
        (
            CodePatch {
                patch_id: "desc_trick".to_string(),
                action: RefactorPlanAction::MoveDependency {
                    from: "adapter".to_string(),
                    to: "controller".to_string(),
                    via: None,
                },
                operations: vec![PatchOperation::UpdateDependency {
                    from: "adapter".to_string(),
                    to: "controller".to_string(),
                    via: None,
                }],
                description: "nl goal routing fix for adapter controller".to_string(),
                target_file: Default::default(),
            },
            vec!["nl", "goal"],
        ),
        (
            CodePatch {
                patch_id: "desc_app".to_string(),
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
                description: "coding app source_index migration".to_string(),
                target_file: Default::default(),
            },
            vec!["coding", "source_index"],
        ),
    ] {
        assert!(
            !patch_matches_cluster(&patch, &cluster),
            "description must not grant cluster membership: {patch:?}"
        );
    }
}

// ─── Case 1: goal.rs dry-run — adapter/app patch completely eliminated ────────

#[test]
fn goal_rs_rejects_adapter_app_patch() {
    let root = temp_workspace("goal_adapter_app");
    write_project(&root);

    let target = Path::new("src/nl/goal.rs");
    let patches = vec![adapter_app_patch()];

    let change_set = generate_code_change_set_with_target(&root, &patches, Some(target))
        .expect("generate should succeed");

    assert!(
        change_set.changes.is_empty(),
        "adapter_app_interface patch must not produce any changes for goal.rs, got: {:?}",
        change_set
            .changes
            .iter()
            .map(|c| &c.file_path)
            .collect::<Vec<_>>()
    );
}

// ─── Case 2: exact "app" module allowed when target is app.rs ─────────────────

#[test]
fn app_rs_allows_all_patches_via_empty_cluster() {
    // app.rs returns empty cluster → pass-through (no pruning)
    let target = Path::new("apps/cli/src/app.rs");
    let patches = vec![adapter_app_patch(), nl_goal_patch()];
    let kept = prune_patches_for_target(&patches, target);
    // empty cluster → all patches pass
    assert_eq!(
        kept.len(),
        2,
        "empty cluster must pass all patches through: {kept:?}"
    );
}

// ─── Case 3: description accidental hit rejected ──────────────────────────────

#[test]
fn goal_rs_rejects_patch_whose_description_mentions_nl() {
    let root = temp_workspace("goal_desc_hit");
    write_project(&root);

    let target = Path::new("src/nl/goal.rs");
    let patches = vec![CodePatch {
        patch_id: "desc_nl_hit".to_string(),
        action: RefactorPlanAction::MoveDependency {
            from: "adapter".to_string(),
            to: "controller".to_string(),
            via: None,
        },
        operations: vec![PatchOperation::UpdateDependency {
            from: "adapter".to_string(),
            to: "controller".to_string(),
            via: None,
        }],
        description: "nl goal routing update".to_string(),
        target_file: Default::default(),
    }];

    let change_set = generate_code_change_set_with_target(&root, &patches, Some(target))
        .expect("generate should succeed");

    assert!(
        change_set.changes.is_empty(),
        "description-only hit must be pruned for goal.rs, got: {:?}",
        change_set
            .changes
            .iter()
            .map(|c| &c.file_path)
            .collect::<Vec<_>>()
    );
}
