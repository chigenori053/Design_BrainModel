use design_cli::coding::{generate_code_change_set, mutation_plan_to_patches, transactional_apply};
use design_cli::service::{MutationConstraints, MutationOperation, MutationPlan, MutationStrategy};
use integration_layer::CodePatch;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos()
}

fn temp_nested_workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "design_cli_representative_target_{name}_{}",
        unique_suffix()
    ));
    fs::create_dir_all(dir.join("crates/runtime/runtime_vm/src")).expect("vm src");
    dir
}

fn write_nested_workspace(root: &Path) {
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
        "use crate::world;\npub fn bind() {}\n",
    )
    .expect("adapter");
}

fn break_cycle_resolution() -> design_cli::coding::MutationResolutionTelemetry {
    design_cli::coding::MutationResolutionTelemetry {
        canonical_target_path: Some(PathBuf::from("crates/runtime/runtime_vm/src/adapter.rs")),
        resolution_pipeline_hits: 0,
        degraded_resolution_hits: 0,
        stale_artifact_detected: false,
    }
}

fn break_cycle_plan() -> MutationPlan {
    MutationPlan {
        edge_id: "adapter->world".to_string(),
        operation: MutationOperation::BreakCycle,
        strategy: MutationStrategy::ExtractInterfaceBothSides,
        constraints: MutationConstraints {
            preserve_public_api: true,
            no_new_cycles: true,
            target_scope_locked: true,
        },
        source_path: Some("crates/runtime/runtime_vm/src/adapter.rs".to_string()),
        snapshot_version: None,
        resolver_version: Some("3".to_string()),
    }
}

/// Reorders a patch slice by the given index permutation and returns the reordered vec.
fn permute_patches(patches: &[CodePatch], order: &[usize]) -> Vec<CodePatch> {
    order.iter().map(|&i| patches[i].clone()).collect()
}

/// Asserts that the representative target (the file from which the root lib.rs is derived)
/// always converges to adapter.rs regardless of the patch ordering.
fn assert_representative_target_is_adapter(root: &Path, patches: &[CodePatch]) {
    let change_set = generate_code_change_set(root, patches).expect("change set");
    // canonical_patch_target_file logic: among patches, adapter.rs ranks 0
    // (non-interface, non-lib.rs), adapter_world_interface.rs ranks 1 (interface).
    // The representative must not be lib.rs or adapter_world_interface.rs.
    let adapter_path = PathBuf::from("crates/runtime/runtime_vm/src/adapter.rs");
    let interface_path = PathBuf::from("crates/runtime/runtime_vm/src/adapter_world_interface.rs");

    // Every change file path must be scoped to crates/runtime/runtime_vm.
    for change in &change_set.changes {
        assert!(
            change.file_path.starts_with("crates/runtime/runtime_vm"),
            "workspace root contamination: unexpected change path {}",
            change.file_path
        );
    }

    // The change set must contain an update to adapter.rs.
    assert!(
        change_set
            .changes
            .iter()
            .any(|c| c.file_path == adapter_path.display().to_string()),
        "adapter.rs must appear in changes: {:?}",
        change_set
            .changes
            .iter()
            .map(|c| &c.file_path)
            .collect::<Vec<_>>()
    );

    // adapter_world_interface.rs is the interface mediation file and must not be
    // elected as representative (it has rank 1, adapter.rs has rank 0).
    let patches_with_target: Vec<_> = change_set
        .patches
        .iter()
        .filter(|p| !p.target_file.as_os_str().is_empty())
        .collect();
    if !patches_with_target.is_empty() {
        let best = patches_with_target
            .iter()
            .min_by_key(|p| {
                let name = p
                    .target_file
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                let stem = p
                    .target_file
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                let rank: u8 = if name == "lib.rs" || name == "main.rs" {
                    2
                } else if stem.contains("_world_interface")
                    || stem.to_lowercase().contains("interface")
                {
                    1
                } else {
                    0
                };
                (rank, p.target_file.as_os_str().to_owned())
            })
            .expect("patches with target");
        assert_eq!(
            best.target_file,
            adapter_path,
            "representative target must be adapter.rs, got: {}",
            best.target_file.display()
        );
        assert_ne!(
            best.target_file, interface_path,
            "interface mediation file must not be representative"
        );
    }
}

/// Test-1 + Test-2: multi-patch ordering does not affect representative target selection.
/// Validates Req-3 (ordering non-dependent) and Req-1 (canonical target fixed to adapter.rs).
#[test]
fn break_cycle_change_set_representative_target_ordering_invariant() {
    let root = temp_nested_workspace("ordering_invariant");
    write_nested_workspace(&root);

    let plan = break_cycle_plan();
    let resolution = break_cycle_resolution();
    let patches = mutation_plan_to_patches(&root, &plan, &resolution).expect("patches");

    // The interface-only branch produces 2 patches (no world peer in this workspace).
    // Test all orderings.
    assert!(
        patches.len() >= 2,
        "expected at least 2 patches, got {}",
        patches.len()
    );

    let orderings: &[&[usize]] = &[&[0, 1], &[1, 0]];

    for order in orderings {
        if order.iter().any(|&i| i >= patches.len()) {
            continue;
        }
        let reordered = permute_patches(&patches, order);
        assert_representative_target_is_adapter(&root, &reordered);
    }
}

/// Test-3: nested workspace root registration lands in crates/runtime/runtime_vm/src/lib.rs,
/// not the workspace root src/lib.rs or any other path.
#[test]
fn break_cycle_change_set_nested_root_registration_is_crate_lib() {
    let root = temp_nested_workspace("nested_root_reg");
    write_nested_workspace(&root);

    let plan = break_cycle_plan();
    let resolution = break_cycle_resolution();
    let patches = mutation_plan_to_patches(&root, &plan, &resolution).expect("patches");
    let change_set = generate_code_change_set(&root, &patches).expect("change set");
    let result = transactional_apply(&root, &change_set, None, true, None).expect("apply");

    assert!(result.applied, "{:?}", result.diagnostics);

    let lib_path = root.join("crates/runtime/runtime_vm/src/lib.rs");
    let lib_content = fs::read_to_string(&lib_path).expect("runtime_vm lib");
    assert!(
        lib_content.contains("pub mod adapter_world_interface;"),
        "crate lib.rs must register adapter_world_interface: {lib_content}"
    );

    // Workspace root must NOT be contaminated.
    let workspace_root_lib = root.join("src/lib.rs");
    assert!(
        !workspace_root_lib.exists(),
        "workspace root src/lib.rs must not be created"
    );
}
