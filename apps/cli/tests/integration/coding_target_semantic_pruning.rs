use std::path::Path;

use design_cli::coding::{
    patch_matches_cluster, prune_patches_for_target, semantic_cluster_for_target,
};
use integration_layer::{CodePatch, PatchOperation, RefactorPlanAction};

fn move_patch(id: &str, from: &str, to: &str) -> CodePatch {
    CodePatch {
        patch_id: id.to_string(),
        action: RefactorPlanAction::MoveDependency {
            from: from.to_string(),
            to: to.to_string(),
            via: None,
        },
        operations: vec![PatchOperation::UpdateDependency {
            from: from.to_string(),
            to: to.to_string(),
            via: None,
        }],
        description: format!("move {} -> {}", from, to),
        target_file: Default::default(),
    }
}

fn introduce_patch(id: &str, a: &str, b: &str) -> CodePatch {
    CodePatch {
        patch_id: id.to_string(),
        action: RefactorPlanAction::IntroduceInterface {
            between: (a.to_string(), b.to_string()),
        },
        operations: vec![PatchOperation::CreateInterface {
            name: format!("{}{}Interface", a, b),
            between: (a.to_string(), b.to_string()),
        }],
        description: format!("introduce interface between {} and {}", a, b),
        target_file: Default::default(),
    }
}

// ─── semantic_cluster_for_target ─────────────────────────────────────────────

#[test]
fn cluster_for_nl_goal() {
    let cluster = semantic_cluster_for_target(Path::new("apps/cli/src/nl/goal.rs"));
    assert!(cluster.contains(&"nl"), "{cluster:?}");
    assert!(cluster.contains(&"goal"), "{cluster:?}");
    // "app" must NOT be in the nl cluster (would cause adapter_app_interface leakage)
    assert!(!cluster.contains(&"app"), "{cluster:?}");
}

#[test]
fn cluster_for_coding_rs() {
    let cluster = semantic_cluster_for_target(Path::new("apps/cli/src/coding.rs"));
    assert!(cluster.contains(&"coding"), "{cluster:?}");
    assert!(cluster.contains(&"source_index"), "{cluster:?}");
}

#[test]
fn cluster_for_agent_mod() {
    let cluster = semantic_cluster_for_target(Path::new("crates/agent_core/src/agent/mod.rs"));
    assert!(cluster.contains(&"agent"), "{cluster:?}");
    assert!(cluster.contains(&"domain"), "{cluster:?}");
    assert!(cluster.contains(&"capability"), "{cluster:?}");
}

#[test]
fn cluster_for_unknown_returns_empty() {
    let cluster = semantic_cluster_for_target(Path::new("apps/cli/src/renderer.rs"));
    assert!(cluster.is_empty(), "{cluster:?}");
}

// ─── patch_matches_cluster ────────────────────────────────────────────────────

#[test]
fn patch_with_nl_from_matches_nl_cluster() {
    // "nl::goal" splits to ["nl", "goal"] — "nl" exactly matches cluster keyword
    let patch = move_patch("p1", "nl::goal", "renderer");
    assert!(patch_matches_cluster(&patch, &["nl", "goal"]));
}

#[test]
fn unrelated_patch_does_not_match_nl_cluster() {
    let patch = introduce_patch("p2", "adapter", "controller");
    assert!(!patch_matches_cluster(&patch, &["nl", "goal"]));
}

#[test]
fn empty_cluster_always_passes() {
    let patch = introduce_patch("p3", "adapter", "controller");
    assert!(patch_matches_cluster(&patch, &[]));
}

// ─── Case 1: goal.rs — unrelated patches are pruned ─────────────────────────

#[test]
fn goal_rs_target_prunes_unrelated_patches() {
    let target = Path::new("apps/cli/src/nl/goal.rs");

    let patches = vec![
        introduce_patch("p1", "adapter", "app"),
        introduce_patch("p2", "controller", "determinism"),
        introduce_patch("p3", "dependency", "engine"),
        move_patch("p4", "agent", "capability"),
        move_patch("p5", "agent", "domain"),
    ];

    let kept = prune_patches_for_target(&patches, target);
    // Cluster is ["nl", "goal"] — adapter/app/controller/dependency/agent/domain
    // are all outside the cluster; all five patches must be pruned.
    let ids: Vec<&str> = kept.iter().map(|p| p.patch_id.as_str()).collect();
    assert!(
        ids.is_empty(),
        "all unrelated patches must be pruned, got: {ids:?}"
    );
}

// ─── Case 2: agent/mod.rs — cross-cluster leakage removed ────────────────────

#[test]
fn agent_mod_prunes_adapter_and_controller_patches() {
    let target = Path::new("crates/agent_core/src/agent/mod.rs");

    let patches = vec![
        introduce_patch("p1", "adapter", "app"),
        introduce_patch("p2", "controller", "determinism"),
        move_patch("p3", "agent", "capability"),
        move_patch("p4", "agent", "domain"),
    ];

    let kept = prune_patches_for_target(&patches, target);
    let ids: Vec<&str> = kept.iter().map(|p| p.patch_id.as_str()).collect();

    // adapter/controller patches must be gone
    assert!(
        !ids.contains(&"p1"),
        "adapter patch should be pruned: {ids:?}"
    );
    assert!(
        !ids.contains(&"p2"),
        "controller patch should be pruned: {ids:?}"
    );

    // agent patches must survive
    assert!(
        ids.contains(&"p3"),
        "agent->capability patch must survive: {ids:?}"
    );
    assert!(
        ids.contains(&"p4"),
        "agent->domain patch must survive: {ids:?}"
    );
}

// ─── Case 3: coding.rs self-host — unrelated patches are empty ───────────────

#[test]
fn coding_rs_self_host_prunes_non_coding_patches() {
    let target = Path::new("apps/cli/src/coding.rs");

    // use modules entirely outside the coding cluster ["coding", "app", "source_index"]
    let patches = vec![
        introduce_patch("p1", "adapter", "controller"),
        introduce_patch("p2", "determinism", "engine"),
        move_patch("p3", "agent", "capability"),
        move_patch("p4", "nl", "goal"),
    ];

    let kept = prune_patches_for_target(&patches, target);
    let ids: Vec<&str> = kept.iter().map(|p| p.patch_id.as_str()).collect();

    // none of adapter/controller/determinism/engine/agent/capability/nl/goal
    // are in the coding cluster
    assert!(
        !ids.contains(&"p1"),
        "adapter/controller patch should be pruned: {ids:?}"
    );
    assert!(
        !ids.contains(&"p2"),
        "determinism/engine patch should be pruned: {ids:?}"
    );
    assert!(
        !ids.contains(&"p3"),
        "agent patch should be pruned: {ids:?}"
    );
    assert!(!ids.contains(&"p4"), "nl patch should be pruned: {ids:?}");
}

#[test]
fn coding_rs_self_host_keeps_coding_patches() {
    let target = Path::new("apps/cli/src/coding.rs");

    let patches = vec![
        move_patch("p1", "coding", "source_index"),
        move_patch("p2", "app", "coding"),
    ];

    let kept = prune_patches_for_target(&patches, target);
    let ids: Vec<&str> = kept.iter().map(|p| p.patch_id.as_str()).collect();

    assert!(
        ids.contains(&"p1"),
        "coding->source_index must survive: {ids:?}"
    );
    assert!(ids.contains(&"p2"), "app->coding must survive: {ids:?}");
}

// ─── unknown target passes through unchanged ──────────────────────────────────

#[test]
fn unknown_target_does_not_prune_any_patches() {
    let target = Path::new("apps/cli/src/renderer.rs");

    let patches = vec![
        introduce_patch("p1", "adapter", "app"),
        move_patch("p2", "agent", "capability"),
    ];

    let kept = prune_patches_for_target(&patches, target);
    // empty cluster -> pass-through
    assert_eq!(kept.len(), 2);
}
