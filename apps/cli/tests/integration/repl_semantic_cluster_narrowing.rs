/// Phase G3.0b — repl.rs Semantic Cluster Narrowing
///
/// Verifies that `repl.rs` target uses cluster ["repl", "nl", "planner_v2"]
/// and rejects broad architectural patches (adapter/app/agent/domain/engine/
/// dependency/controller/capability) while allowing planner_v2 wiring patches.
use std::path::Path;

use design_cli::coding::{
    patch_matches_cluster, prune_patches_for_target, semantic_cluster_for_target,
};
use integration_layer::{CodePatch, PatchOperation, RefactorPlanAction};

// ─── helpers ─────────────────────────────────────────────────────────────────

fn adapter_app_patch() -> CodePatch {
    CodePatch {
        patch_id: "adapter_app_interface".to_string(),
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

fn agent_domain_patch() -> CodePatch {
    CodePatch {
        patch_id: "agent_domain_interface".to_string(),
        action: RefactorPlanAction::IntroduceInterface {
            between: ("agent".to_string(), "domain".to_string()),
        },
        operations: vec![PatchOperation::CreateInterface {
            name: "agent_domain_interface".to_string(),
            between: ("agent".to_string(), "domain".to_string()),
        }],
        description: "introduce agent-domain boundary".to_string(),
        target_file: Default::default(),
    }
}

fn dependency_engine_patch() -> CodePatch {
    CodePatch {
        patch_id: "dependency_engine_interface".to_string(),
        action: RefactorPlanAction::MoveDependency {
            from: "dependency".to_string(),
            to: "engine".to_string(),
            via: None,
        },
        operations: vec![PatchOperation::UpdateDependency {
            from: "dependency".to_string(),
            to: "engine".to_string(),
            via: None,
        }],
        description: "move dependency -> engine".to_string(),
        target_file: Default::default(),
    }
}

fn controller_determinism_patch() -> CodePatch {
    CodePatch {
        patch_id: "controller_determinism".to_string(),
        action: RefactorPlanAction::MoveDependency {
            from: "controller".to_string(),
            to: "determinism".to_string(),
            via: None,
        },
        operations: vec![PatchOperation::UpdateDependency {
            from: "controller".to_string(),
            to: "determinism".to_string(),
            via: None,
        }],
        description: "move controller -> determinism".to_string(),
        target_file: Default::default(),
    }
}

fn planner_v2_wiring_patch() -> CodePatch {
    CodePatch {
        patch_id: "planner_v2_wiring".to_string(),
        action: RefactorPlanAction::MoveDependency {
            from: "repl".to_string(),
            to: "nl::planner_v2".to_string(),
            via: None,
        },
        operations: vec![PatchOperation::UpdateDependency {
            from: "repl".to_string(),
            to: "nl::planner_v2".to_string(),
            via: None,
        }],
        description: "wire repl to planner_v2".to_string(),
        target_file: Default::default(),
    }
}

fn nl_repl_patch() -> CodePatch {
    CodePatch {
        patch_id: "nl_repl_wiring".to_string(),
        action: RefactorPlanAction::MoveDependency {
            from: "nl".to_string(),
            to: "repl".to_string(),
            via: None,
        },
        operations: vec![PatchOperation::UpdateDependency {
            from: "nl".to_string(),
            to: "repl".to_string(),
            via: None,
        }],
        description: "wire nl -> repl".to_string(),
        target_file: Default::default(),
    }
}

// ─── Case 1: cluster shape for repl.rs ───────────────────────────────────────

#[test]
fn repl_rs_cluster_contains_repl_nl_planner_v2() {
    let cluster = semantic_cluster_for_target(Path::new("apps/cli/src/repl.rs"));
    assert!(
        cluster.contains(&"repl"),
        "\"repl\" must be in repl.rs cluster: {cluster:?}"
    );
    assert!(
        cluster.contains(&"nl"),
        "\"nl\" must be in repl.rs cluster: {cluster:?}"
    );
    assert!(
        cluster.contains(&"planner_v2"),
        "\"planner_v2\" must be in repl.rs cluster: {cluster:?}"
    );
}

#[test]
fn repl_rs_cluster_excludes_broad_architectural_tokens() {
    let cluster = semantic_cluster_for_target(Path::new("apps/cli/src/repl.rs"));
    for forbidden in &[
        "app",
        "adapter",
        "agent",
        "dependency",
        "controller",
        "domain",
        "engine",
        "capability",
    ] {
        assert!(
            !cluster.contains(forbidden),
            "broad token \"{forbidden}\" must NOT be in repl.rs cluster: {cluster:?}"
        );
    }
}

// ─── Case 1: adapter/app rejected ────────────────────────────────────────────

#[test]
fn repl_rs_prunes_broad_architectural_patches() {
    let target = Path::new("apps/cli/src/repl.rs");
    for patch in [
        adapter_app_patch(),
        agent_domain_patch(),
        dependency_engine_patch(),
        controller_determinism_patch(),
    ] {
        let kept = prune_patches_for_target(&[patch], target);
        assert!(
            kept.is_empty(),
            "broad architectural patch must be pruned for repl.rs, kept: {kept:?}"
        );
    }
}

// ─── Case 2: planner_v2 import allowed ───────────────────────────────────────

#[test]
fn repl_rs_allows_repl_local_wiring_patches() {
    let target = Path::new("apps/cli/src/repl.rs");
    for patch in [planner_v2_wiring_patch(), nl_repl_patch()] {
        let kept = prune_patches_for_target(&[patch], target);
        assert_eq!(
            kept.len(),
            1,
            "repl-local wiring patch must pass through for repl.rs, kept: {kept:?}"
        );
    }
}

#[test]
fn planner_v2_namespaced_import_matches_repl_cluster() {
    // `crate::nl::planner_v2` path splits to ["crate", "nl", "planner_v2"]
    // both "nl" and "planner_v2" are in the repl cluster → must match
    let patch = CodePatch {
        patch_id: "planner_v2_import".to_string(),
        action: RefactorPlanAction::MoveDependency {
            from: "repl".to_string(),
            to: "crate::nl::planner_v2".to_string(),
            via: None,
        },
        operations: vec![PatchOperation::UpdateDependency {
            from: "repl".to_string(),
            to: "crate::nl::planner_v2".to_string(),
            via: None,
        }],
        description: "use crate::nl::planner_v2".to_string(),
        target_file: Default::default(),
    };
    assert!(
        patch_matches_cluster(&patch, &["repl", "nl", "planner_v2"]),
        "crate::nl::planner_v2 import must match repl cluster"
    );
}

// ─── Broad token exact match — compound names must not leak ──────────────────

#[test]
fn adapter_app_compound_name_does_not_match_repl_cluster() {
    // "adapter_app_interface" is a compound name — exact segment matching
    // must not split on "_" — only "::" is the delimiter
    let patch = CodePatch {
        patch_id: "compound_leak".to_string(),
        action: RefactorPlanAction::IntroduceInterface {
            between: ("adapter".to_string(), "app".to_string()),
        },
        operations: vec![PatchOperation::CreateInterface {
            name: "adapter_app_interface".to_string(),
            between: ("adapter".to_string(), "app".to_string()),
        }],
        description: "accidental compound name".to_string(),
        target_file: Default::default(),
    };
    assert!(
        !patch_matches_cluster(&patch, &["repl", "nl", "planner_v2"]),
        "adapter_app_interface compound name must not match repl cluster"
    );
}

// ─── Mixed batch: architectural patches pruned, wiring patches retained ──────

#[test]
fn mixed_batch_prunes_architectural_retains_wiring() {
    let target = Path::new("apps/cli/src/repl.rs");
    let patches = vec![
        adapter_app_patch(),
        agent_domain_patch(),
        dependency_engine_patch(),
        planner_v2_wiring_patch(),
        nl_repl_patch(),
    ];
    let kept = prune_patches_for_target(&patches, target);
    assert_eq!(
        kept.len(),
        2,
        "only planner_v2 and nl->repl patches must survive pruning, kept: {:?}",
        kept.iter().map(|p| &p.patch_id).collect::<Vec<_>>()
    );
    let ids: Vec<&str> = kept.iter().map(|p| p.patch_id.as_str()).collect();
    assert!(
        ids.contains(&"planner_v2_wiring"),
        "planner_v2_wiring must be retained"
    );
    assert!(
        ids.contains(&"nl_repl_wiring"),
        "nl_repl_wiring must be retained"
    );
}
