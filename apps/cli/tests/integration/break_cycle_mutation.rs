use design_cli::coding::{
    derive_break_cycle_peer_target, generate_code_change_set, mutation_plan_to_patches,
    transactional_apply,
};
use design_cli::service::{MutationConstraints, MutationOperation, MutationPlan, MutationStrategy};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("design_cli_break_cycle_{name}_{unique}"));
    fs::create_dir_all(dir.join("src/adapter")).expect("src adapter");
    fs::create_dir_all(dir.join("src/world")).expect("src world");
    fs::create_dir_all(dir.join("src/ports")).expect("src ports");
    dir
}

fn write_workspace(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"break_cycle_mutation\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        root.join("src/lib.rs"),
        "pub mod adapter;\npub mod ports;\npub mod world;\n",
    )
    .expect("lib");
    fs::write(
        root.join("src/adapter/mod.rs"),
        "use crate::world;\n\npub fn bind() {\n    let _ = 1usize;\n}\n",
    )
    .expect("adapter");
    fs::write(
        root.join("src/world/mod.rs"),
        "use crate::adapter;\n\npub fn ping() {\n    let _ = 1usize;\n}\n",
    )
    .expect("world");
    fs::write(root.join("src/ports/mod.rs"), "pub fn ping() {}\n").expect("ports");
}

fn write_mutation_plan(path: &Path) {
    fs::create_dir_all(path.parent().expect("parent")).expect("design dir");
    fs::write(
        path,
        r#"{
  "edge_id": "adapter->world",
  "operation": "break_cycle",
  "strategy": "extract_interface_both_sides",
  "resolver_version": "3",
  "constraints": {
    "preserve_public_api": true,
    "no_new_cycles": true,
    "target_scope_locked": true
  }
}"#,
    )
    .expect("mutation");
}

fn temp_runtime_vm_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir =
        std::env::temp_dir().join(format!("design_cli_break_cycle_runtime_vm_{name}_{unique}"));
    fs::create_dir_all(dir.join("crates/runtime/runtime_vm/src/adapter")).expect("adapter dir");
    fs::create_dir_all(dir.join("crates/runtime/runtime_vm/src/world")).expect("world dir");
    fs::create_dir_all(dir.join("crates/runtime/runtime_vm/src/ports")).expect("ports dir");
    fs::create_dir_all(dir.join("tests/fixtures/demo/src/world")).expect("fixture src");
    dir
}

fn temp_runtime_vm_flat_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "design_cli_break_cycle_runtime_vm_flat_{name}_{unique}"
    ));
    fs::create_dir_all(dir.join("crates/runtime/runtime_vm/src")).expect("runtime_vm src");
    fs::create_dir_all(dir.join("tests/fixtures/demo/src")).expect("fixture src");
    dir
}

fn write_runtime_vm_workspace(root: &Path) {
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
        "pub mod adapter;\npub mod ports;\npub mod world;\n",
    )
    .expect("lib");
    fs::write(
        root.join("crates/runtime/runtime_vm/src/adapter/mod.rs"),
        "use crate::world;\n\npub fn bind() {\n    let _ = 1usize;\n}\n",
    )
    .expect("adapter");
    fs::write(
        root.join("crates/runtime/runtime_vm/src/world/mod.rs"),
        "use crate::adapter;\n\npub fn tick() {\n    let _ = 1usize;\n}\n",
    )
    .expect("world");
    fs::write(
        root.join("crates/runtime/runtime_vm/src/ports/mod.rs"),
        "pub fn ping() {}\n",
    )
    .expect("ports");
    fs::write(
        root.join("tests/fixtures/demo/src/world/mod.rs"),
        "use crate::adapter;\npub fn fixture_world() {}\n",
    )
    .expect("fixture world");
}

fn write_runtime_vm_flat_workspace(root: &Path) {
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
        "pub mod adapter;\npub mod world;\n",
    )
    .expect("lib");
    fs::write(
        root.join("crates/runtime/runtime_vm/src/adapter.rs"),
        "use crate::world;\npub fn bind() {}\n",
    )
    .expect("adapter");
    fs::write(
        root.join("crates/runtime/runtime_vm/src/world.rs"),
        "use crate::adapter;\npub fn tick() {}\n",
    )
    .expect("world");
    fs::write(
        root.join("tests/fixtures/demo/src/world.rs"),
        "use crate::adapter;\npub fn fixture_world() {}\n",
    )
    .expect("fixture world");
}

fn temp_missing_world_peer_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "design_cli_break_cycle_missing_peer_{name}_{unique}"
    ));
    fs::create_dir_all(dir.join("src")).expect("src dir");
    fs::create_dir_all(dir.join("tests/fixtures/demo/src/world")).expect("fixture world dir");
    dir
}

fn write_missing_world_peer_workspace(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"break_cycle_missing_peer\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        root.join("src/lib.rs"),
        "pub mod adapter;\npub mod adapter_world_interface;\n",
    )
    .expect("lib");
    fs::write(
        root.join("src/adapter.rs"),
        "use crate::world;\n\npub fn bind() {\n    let _ = 1usize;\n}\n",
    )
    .expect("adapter");
    fs::write(
        root.join("tests/fixtures/demo/src/world/mod.rs"),
        "use crate::adapter;\npub fn fixture_world() {}\n",
    )
    .expect("fixture world");
}

fn write_semantic_preservation_workspace(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"break_cycle_semantic_preservation\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("cargo");
    fs::write(
        root.join("src/lib.rs"),
        "pub mod adapter;\npub mod adapter_world_interface;\n",
    )
    .expect("lib");
    fs::write(
        root.join("src/adapter.rs"),
        r#"use knowledge_engine::{KnowledgeGraph, ValidationScore};
use knowledge_lifecycle::KnowledgeLifecycleState;
use crate::world;

pub fn bind(reasoned_language_state: Option<()>) {
    let (
        knowledge_graph,
        inferred_knowledge_graph,
        lifecycle_state,
        knowledge_validation,
        knowledge_retrieved,
    ) = if let Some(_state) = reasoned_language_state.as_ref() {
        (
            KnowledgeGraph::default(),
            KnowledgeGraph::default(),
            KnowledgeLifecycleState::default(),
            ValidationScore::default(),
            true,
        )
    } else {
        (
            KnowledgeGraph::default(),
            KnowledgeLifecycleState::default(),
            ValidationScore::default(),
            false,
        )
    };
    let _ = (
        knowledge_graph,
        inferred_knowledge_graph,
        lifecycle_state,
        knowledge_validation,
        knowledge_retrieved,
    );
}
"#,
    )
    .expect("adapter");
}

fn temp_controller_replay_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "design_cli_break_cycle_controller_replay_{name}_{unique}"
    ));
    fs::create_dir_all(dir.join("crates/execution_stability_core/src/controller"))
        .expect("controller dir");
    fs::create_dir_all(dir.join("crates/execution_stability_core/src/replay")).expect("replay dir");
    dir
}

fn write_controller_replay_workspace(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/execution_stability_core\"]\nresolver = \"2\"\n",
    )
    .expect("workspace cargo");
    fs::write(
        root.join("crates/execution_stability_core/Cargo.toml"),
        "[package]\nname = \"execution_stability_core\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("crate cargo");
    fs::write(
        root.join("crates/execution_stability_core/src/lib.rs"),
        "pub mod controller;\npub mod replay;\n",
    )
    .expect("lib");
    fs::write(
        root.join("crates/execution_stability_core/src/controller/mod.rs"),
        "pub mod execution_controller;\n",
    )
    .expect("controller mod");
    fs::write(
        root.join("crates/execution_stability_core/src/replay/mod.rs"),
        "pub mod replay_engine;\n",
    )
    .expect("replay mod");
    fs::write(
        root.join("crates/execution_stability_core/src/replay/replay_engine.rs"),
        "pub trait ReplayEngine {\n    fn replay(&self) -> bool;\n}\n\n#[derive(Clone, Debug, Default)]\npub struct DefaultReplayEngine;\n\nimpl ReplayEngine for DefaultReplayEngine {\n    fn replay(&self) -> bool {\n        true\n    }\n}\n",
    )
    .expect("replay engine");
    fs::write(
        root.join("crates/execution_stability_core/src/controller/execution_controller.rs"),
        "use crate::replay::replay_engine::{DefaultReplayEngine, ReplayEngine};\n\n#[derive(Clone, Debug, Default)]\npub struct DefaultExecutionController {\n    pub replay_engine: DefaultReplayEngine,\n}\n\nimpl DefaultExecutionController {\n    pub fn execute_with_control(&self) -> bool {\n        self.replay_engine.replay()\n    }\n}\n",
    )
    .expect("execution controller");
}

fn temp_runtime_vm_real_adapter_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "design_cli_break_cycle_runtime_vm_real_{name}_{unique}"
    ));
    fs::create_dir_all(dir.join("crates/runtime/runtime_vm/src")).expect("runtime_vm src");
    fs::create_dir_all(dir.join("crates/knowledge_engine/src")).expect("knowledge_engine src");
    fs::create_dir_all(dir.join("crates/knowledge_lifecycle/src"))
        .expect("knowledge_lifecycle src");
    fs::create_dir_all(dir.join("tests/fixtures/demo/src/world")).expect("fixture world dir");
    dir
}

fn temp_runtime_vm_missing_world_peer_workspace(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "design_cli_break_cycle_runtime_vm_missing_peer_{name}_{unique}"
    ));
    fs::create_dir_all(dir.join("src")).expect("workspace src");
    fs::create_dir_all(dir.join("crates/runtime/runtime_vm/src")).expect("runtime_vm src");
    fs::create_dir_all(dir.join("tests/fixtures/demo/src/world")).expect("fixture world dir");
    dir
}

fn write_runtime_vm_real_adapter_workspace(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/runtime/runtime_vm\", \"crates/knowledge_engine\", \"crates/knowledge_lifecycle\"]\nresolver = \"2\"\n",
    )
    .expect("workspace cargo");
    fs::write(
        root.join("crates/runtime/runtime_vm/Cargo.toml"),
        "[package]\nname = \"runtime_vm\"\nversion = \"0.1.0\"\nedition = \"2024\"\n[dependencies]\nknowledge_engine = { path = \"../../knowledge_engine\" }\nknowledge_lifecycle = { path = \"../../knowledge_lifecycle\" }\n",
    )
    .expect("runtime_vm cargo");
    fs::write(
        root.join("crates/knowledge_engine/Cargo.toml"),
        "[package]\nname = \"knowledge_engine\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("knowledge_engine cargo");
    fs::write(
        root.join("crates/knowledge_lifecycle/Cargo.toml"),
        "[package]\nname = \"knowledge_lifecycle\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("knowledge_lifecycle cargo");
    fs::write(
        root.join("crates/runtime/runtime_vm/src/lib.rs"),
        "pub mod adapter;\n",
    )
    .expect("runtime_vm lib");
    fs::write(
        root.join("crates/runtime/runtime_vm/src/adapter.rs"),
        r#"use knowledge_engine::{KnowledgeGraph, ValidationScore};
use knowledge_lifecycle::KnowledgeLifecycleState;
use crate::world;

pub fn bind(reasoned_language_state: Option<()>) {
    let (
        knowledge_graph,
        inferred_knowledge_graph,
        lifecycle_state,
        knowledge_validation,
        knowledge_retrieved,
    ) = if let Some(_state) = reasoned_language_state.as_ref() {
        (
            KnowledgeGraph::default(),
            KnowledgeLifecycleState::default(),
            ValidationScore::default(),
            true,
        )
    } else {
        (
            KnowledgeGraph::default(),
            KnowledgeLifecycleState::default(),
            ValidationScore::default(),
            false,
        )
    };
    let _ = (
        knowledge_graph,
        inferred_knowledge_graph,
        lifecycle_state,
        knowledge_validation,
        knowledge_retrieved,
    );
}
"#,
    )
    .expect("runtime_vm adapter");
    fs::write(
        root.join("crates/knowledge_engine/src/lib.rs"),
        "#[derive(Default)]\npub struct KnowledgeGraph;\n\n#[derive(Default)]\npub struct ValidationScore;\n",
    )
    .expect("knowledge_engine lib");
    fs::write(
        root.join("crates/knowledge_lifecycle/src/lib.rs"),
        "#[derive(Default)]\npub struct KnowledgeLifecycleState;\n",
    )
    .expect("knowledge_lifecycle lib");
    fs::write(
        root.join("tests/fixtures/demo/src/world/mod.rs"),
        "use crate::adapter;\npub fn fixture_world() {}\n",
    )
    .expect("fixture world");
}

fn write_runtime_vm_missing_world_peer_workspace(root: &Path) {
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/runtime/runtime_vm\"]\nresolver = \"2\"\n",
    )
    .expect("workspace cargo");
    fs::write(root.join("src/lib.rs"), "pub mod workspace_only;\n").expect("workspace lib");
    fs::write(
        root.join("crates/runtime/runtime_vm/Cargo.toml"),
        "[package]\nname = \"runtime_vm\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("runtime_vm cargo");
    fs::write(
        root.join("crates/runtime/runtime_vm/src/lib.rs"),
        "pub mod adapter;\n",
    )
    .expect("runtime_vm lib");
    fs::write(
        root.join("crates/runtime/runtime_vm/src/adapter.rs"),
        "use crate::world;\n\npub fn bind() {\n    let _ = 1usize;\n}\n",
    )
    .expect("runtime_vm adapter");
    fs::write(
        root.join("tests/fixtures/demo/src/world/mod.rs"),
        "use crate::adapter;\npub fn fixture_world() {}\n",
    )
    .expect("fixture world");
}

fn runtime_vm_edge_id(snapshot: &Value) -> String {
    snapshot
        .get("edges")
        .and_then(Value::as_array)
        .and_then(|edges| {
            edges.iter().find_map(|edge| {
                let from = edge.get("from").and_then(Value::as_str)?;
                let to = edge.get("to").and_then(Value::as_str)?;
                ((from == "adapter" || from.ends_with("::adapter"))
                    && (to == "world" || to.ends_with("::world")))
                .then(|| {
                    edge.get("id")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
                .flatten()
            })
        })
        .expect("runtime_vm adapter->world edge")
}

fn write_runtime_vm_break_cycle_plan(path: &Path, edge_id: &str) {
    fs::create_dir_all(path.parent().expect("parent")).expect("design dir");
    fs::write(
        path,
        format!(
            r#"{{
  "edge_id": "{edge_id}",
  "operation": "break_cycle",
  "strategy": "extract_interface_both_sides",
  "resolver_version": "3",
  "constraints": {{
    "preserve_public_api": true,
    "no_new_cycles": true,
    "target_scope_locked": true
  }}
}}"#
        ),
    )
    .expect("mutation");
}

fn run_cli(workspace: &Path, args: &[&str]) -> (i32, String, String) {
    let exe = env!("CARGO_BIN_EXE_design_cli");
    let out = Command::new(exe)
        .args(args)
        .current_dir(workspace)
        .output()
        .expect("run design_cli");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

fn has_adapter_world_cycle(snapshot: &Value) -> bool {
    snapshot
        .get("cycles")
        .and_then(Value::as_array)
        .map(|cycles| {
            cycles.iter().any(|cycle| {
                let nodes = cycle
                    .as_array()
                    .map(|items| items.iter().filter_map(Value::as_str).collect::<Vec<_>>())
                    .unwrap_or_default();
                nodes.windows(2).any(|pair| pair == ["adapter", "world"])
                    && nodes.windows(2).any(|pair| pair == ["world", "adapter"])
            })
        })
        .unwrap_or(false)
}

#[test]
fn break_cycle_mutation_removes_adapter_world_cycle_in_post_snapshot() {
    let root = temp_workspace("post_snapshot");
    write_workspace(&root);
    write_mutation_plan(&root.join(".dbm/design/mutation.json"));

    let (before_code, before_stdout, before_stderr) =
        run_cli(&root, &["analyze", ".", "--design-json"]);
    assert_eq!(before_code, 0, "stderr: {before_stderr}");
    let before: Value = serde_json::from_str(&before_stdout).expect("before json");
    assert!(
        has_adapter_world_cycle(&before),
        "expected adapter/world cycle before apply: {before_stdout}"
    );

    let (apply_code, apply_stdout, apply_stderr) = run_cli(
        &root,
        &[
            "coding",
            ".",
            "--from-design-snapshot",
            ".dbm/design/mutation.json",
            "--apply",
            "--json",
        ],
    );
    assert_eq!(apply_code, 0, "stderr: {apply_stderr}");
    let apply: Value = serde_json::from_str(&apply_stdout).expect("apply json");
    assert_eq!(
        apply
            .get("execution")
            .and_then(|value| value.get("applied"))
            .and_then(Value::as_bool),
        Some(true),
        "stdout: {apply_stdout}"
    );
    assert!(
        apply
            .get("changes")
            .and_then(|value| value.get("summary"))
            .and_then(|value| value.get("total_changes"))
            .and_then(Value::as_u64)
            .unwrap_or_default()
            >= 4,
        "stdout: {apply_stdout}"
    );

    let (after_code, after_stdout, after_stderr) =
        run_cli(&root, &["analyze", ".", "--design-json"]);
    assert_eq!(after_code, 0, "stderr: {after_stderr}");
    let after: Value = serde_json::from_str(&after_stdout).expect("after json");
    assert!(
        !has_adapter_world_cycle(&after),
        "expected adapter/world cycle removed after apply: {after_stdout}"
    );
}

#[test]
fn break_cycle_runtime_vm_avoids_stale_warning_and_fixture_paths() {
    let root = temp_runtime_vm_workspace("resolver_guard");
    write_runtime_vm_workspace(&root);
    let (analyze_code, analyze_stdout, analyze_stderr) =
        run_cli(&root, &["analyze", ".", "--design-json"]);
    assert_eq!(analyze_code, 0, "stderr: {analyze_stderr}");
    let snapshot: Value = serde_json::from_str(&analyze_stdout).expect("analyze json");
    let edge_id = runtime_vm_edge_id(&snapshot);
    write_runtime_vm_break_cycle_plan(&root.join(".dbm/design/mutation.json"), &edge_id);

    let (code, stdout, stderr) = run_cli(
        &root,
        &[
            "coding",
            ".",
            "--from-design-snapshot",
            ".dbm/design/mutation.json",
            "--check",
            "--json",
        ],
    );
    assert_eq!(code, 0, "stderr: {stderr}");
    let parsed: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(
        parsed
            .get("execution")
            .and_then(|value| value.get("stale_artifact_detected"))
            .and_then(Value::as_bool),
        Some(false),
        "stdout: {stdout}"
    );
    let reason = parsed
        .get("execution")
        .and_then(|value| value.get("reason"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    assert!(
        !reason.contains("stale snapshot artifact detected"),
        "stdout: {stdout}"
    );
    assert_eq!(
        parsed
            .get("execution")
            .and_then(|value| value.get("fallback_resolution_hits"))
            .and_then(Value::as_u64),
        Some(0),
        "stdout: {stdout}"
    );
    let resolutions = parsed
        .get("apply_resolutions")
        .and_then(Value::as_array)
        .expect("apply resolutions");
    assert!(
        resolutions.iter().all(|value| {
            !value
                .get("resolved_relative_path")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .contains("fixtures")
        }),
        "stdout: {stdout}"
    );
    assert!(
        resolutions.iter().any(|value| {
            value.get("module").and_then(Value::as_str) == Some("world")
                && value.get("resolved_relative_path").and_then(Value::as_str)
                    == Some("crates/runtime/runtime_vm/src/world/mod.rs")
        }),
        "stdout: {stdout}"
    );
}

#[test]
fn break_cycle_peer_target_propagation() {
    let root = temp_runtime_vm_flat_workspace("peer_target");
    write_runtime_vm_flat_workspace(&root);
    let canonical_target_file = PathBuf::from("crates/runtime/runtime_vm/src/adapter.rs");
    let peer = derive_break_cycle_peer_target(&root, &canonical_target_file, "world")
        .expect("peer target from canonical_target_file");

    assert_eq!(
        canonical_target_file,
        PathBuf::from("crates/runtime/runtime_vm/src/adapter.rs")
    );
    assert_eq!(
        peer,
        PathBuf::from("crates/runtime/runtime_vm/src/world.rs")
    );
    assert!(!peer.to_string_lossy().contains("fixtures"));
}

#[test]
fn break_cycle_missing_world_peer_layout_uses_interface_only_branch() {
    let root = temp_missing_world_peer_workspace("interface_only");
    write_missing_world_peer_workspace(&root);
    let plan = MutationPlan {
        edge_id: "adapter->world".to_string(),
        operation: MutationOperation::BreakCycle,
        strategy: MutationStrategy::ExtractInterfaceBothSides,
        constraints: MutationConstraints {
            preserve_public_api: true,
            no_new_cycles: true,
            target_scope_locked: true,
        },
        source_path: Some("src/adapter.rs".to_string()),
        snapshot_version: None,
        resolver_version: Some("3".to_string()),
    };
    let resolution = design_cli::coding::MutationResolutionTelemetry {
        canonical_target_path: Some(PathBuf::from("src/adapter.rs")),
        legacy_pipeline_hits: 0,
        fallback_resolution_hits: 0,
        stale_artifact_detected: false,
    };
    let patches = mutation_plan_to_patches(&root, &plan, &resolution).expect("patches");
    assert_eq!(patches.len(), 2, "{patches:?}");
    let change_set = generate_code_change_set(&root, &patches).expect("change set");

    assert_eq!(change_set.summary.total_changes, 2, "{change_set:?}");
    assert!(
        change_set
            .changes
            .iter()
            .any(|change| change.file_path == "src/adapter.rs"),
        "{change_set:?}"
    );
    assert!(
        change_set
            .changes
            .iter()
            .any(|change| change.file_path == "src/adapter_world_interface.rs"),
        "{change_set:?}"
    );
    assert!(
        change_set
            .changes
            .iter()
            .all(|change| !change.file_path.contains("world.rs")),
        "{change_set:?}"
    );
}

#[test]
fn break_cycle_semantic_preservation_integration() {
    let root = temp_missing_world_peer_workspace("semantic_preservation");
    write_semantic_preservation_workspace(&root);
    let plan = MutationPlan {
        edge_id: "adapter->world".to_string(),
        operation: MutationOperation::BreakCycle,
        strategy: MutationStrategy::ExtractInterfaceBothSides,
        constraints: MutationConstraints {
            preserve_public_api: true,
            no_new_cycles: true,
            target_scope_locked: true,
        },
        source_path: Some("src/adapter.rs".to_string()),
        snapshot_version: None,
        resolver_version: Some("3".to_string()),
    };
    let resolution = design_cli::coding::MutationResolutionTelemetry {
        canonical_target_path: Some(PathBuf::from("src/adapter.rs")),
        legacy_pipeline_hits: 0,
        fallback_resolution_hits: 0,
        stale_artifact_detected: false,
    };
    let patches = mutation_plan_to_patches(&root, &plan, &resolution).expect("patches");
    let change_set = generate_code_change_set(&root, &patches).expect("change set");
    let adapter = change_set
        .changes
        .iter()
        .find(|change| change.file_path == "src/adapter.rs")
        .and_then(|change| change.hunks.last())
        .map(|hunk| hunk.replacement.as_str())
        .expect("adapter replacement");

    assert!(
        !adapter.contains("use crate::adapter_world_interface::AdapterWorldInterface;"),
        "{adapter}"
    );
    assert!(
        adapter.contains("let (\n        knowledge_graph,\n        inferred_knowledge_graph,\n        lifecycle_state,\n        knowledge_validation,\n        knowledge_retrieved,"),
        "{adapter}"
    );
    assert!(
        adapter.contains(
            "    ) = if let Some(_state) = reasoned_language_state.as_ref() {\n        (\n            KnowledgeGraph::default(),\n            KnowledgeGraph::default(),\n            KnowledgeLifecycleState::default(),\n            ValidationScore::default(),\n            true,"
        ),
        "{adapter}"
    );
    assert!(
        adapter.contains(
            "    } else {\n        (\n            KnowledgeGraph::default(),\n            KnowledgeGraph::default(),\n            KnowledgeLifecycleState::default(),\n            ValidationScore::default(),\n            false,"
        ),
        "{adapter}"
    );
}

#[test]
fn controller_replay_break_cycle_trait_preservation() {
    let root = temp_controller_replay_workspace("trait_preservation");
    write_controller_replay_workspace(&root);
    let plan = MutationPlan {
        edge_id: "controller->replay".to_string(),
        operation: MutationOperation::BreakCycle,
        strategy: MutationStrategy::ExtractInterfaceBothSides,
        constraints: MutationConstraints {
            preserve_public_api: true,
            no_new_cycles: true,
            target_scope_locked: true,
        },
        source_path: Some(
            "crates/execution_stability_core/src/controller/execution_controller.rs".to_string(),
        ),
        snapshot_version: None,
        resolver_version: Some("3".to_string()),
    };
    let resolution = design_cli::coding::MutationResolutionTelemetry {
        canonical_target_path: Some(PathBuf::from(
            "crates/execution_stability_core/src/controller/execution_controller.rs",
        )),
        legacy_pipeline_hits: 0,
        fallback_resolution_hits: 0,
        stale_artifact_detected: false,
    };
    let patches = mutation_plan_to_patches(&root, &plan, &resolution).expect("patches");
    let change_set = generate_code_change_set(&root, &patches).expect("change set");

    let result = transactional_apply(&root, &change_set, None, false, None).expect("apply");
    assert!(result.applied, "{:?}", result.diagnostics);
    assert!(result.build_ok, "{:?}", result.diagnostics);

    let controller = fs::read_to_string(
        root.join("crates/execution_stability_core/src/controller/execution_controller.rs"),
    )
    .expect("controller");
    assert!(
        controller
            .contains("use crate::replay::replay_engine::{DefaultReplayEngine, ReplayEngine};"),
        "{controller}"
    );
}

#[test]
fn break_cycle_runtime_vm_real_adapter_fixture() {
    let root = temp_runtime_vm_real_adapter_workspace("semantic_finalizer");
    write_runtime_vm_real_adapter_workspace(&root);
    let plan = MutationPlan {
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
    };
    let resolution = design_cli::coding::MutationResolutionTelemetry {
        canonical_target_path: Some(PathBuf::from("crates/runtime/runtime_vm/src/adapter.rs")),
        legacy_pipeline_hits: 0,
        fallback_resolution_hits: 0,
        stale_artifact_detected: false,
    };
    let patches = mutation_plan_to_patches(&root, &plan, &resolution).expect("patches");
    let change_set = generate_code_change_set(&root, &patches).expect("change set");
    let result = transactional_apply(&root, &change_set, None, false, None).expect("apply");

    assert!(result.applied, "{:?}", result.diagnostics);
    assert!(result.build_ok, "{:?}", result.diagnostics);

    let adapter =
        fs::read_to_string(root.join("crates/runtime/runtime_vm/src/adapter.rs")).expect("adapter");
    assert!(
        !adapter.contains("use crate::adapter_world_interface::AdapterWorldInterface;"),
        "{adapter}"
    );
    assert!(
        adapter.contains(
            "    ) = if let Some(_state) = reasoned_language_state.as_ref() {\n        (\n            KnowledgeGraph::default(),\n            KnowledgeGraph::default(),\n            KnowledgeLifecycleState::default(),\n            ValidationScore::default(),\n            true,"
        ),
        "{adapter}"
    );
    assert!(
        adapter.contains(
            "    } else {\n        (\n            KnowledgeGraph::default(),\n            KnowledgeGraph::default(),\n            KnowledgeLifecycleState::default(),\n            ValidationScore::default(),\n            false,"
        ),
        "{adapter}"
    );

    let cargo_check = Command::new("cargo")
        .args(["check", "-p", "runtime_vm"])
        .current_dir(&root)
        .output()
        .expect("cargo check");
    assert!(
        cargo_check.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&cargo_check.stdout),
        String::from_utf8_lossy(&cargo_check.stderr)
    );
}

#[test]
fn break_cycle_workspace_nested_root_resolution() {
    let root = temp_runtime_vm_missing_world_peer_workspace("nested_root");
    write_runtime_vm_missing_world_peer_workspace(&root);
    let plan = MutationPlan {
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
    };
    let resolution = design_cli::coding::MutationResolutionTelemetry {
        canonical_target_path: Some(PathBuf::from("crates/runtime/runtime_vm/src/adapter.rs")),
        legacy_pipeline_hits: 0,
        fallback_resolution_hits: 0,
        stale_artifact_detected: false,
    };
    let patches = mutation_plan_to_patches(&root, &plan, &resolution).expect("patches");
    let change_set = generate_code_change_set(&root, &patches).expect("change set");
    let result = transactional_apply(&root, &change_set, None, true, None).expect("apply");

    assert!(result.applied, "{:?}", result.diagnostics);

    let runtime_vm_lib = fs::read_to_string(root.join("crates/runtime/runtime_vm/src/lib.rs"))
        .expect("runtime_vm lib");
    let workspace_lib = fs::read_to_string(root.join("src/lib.rs")).expect("workspace lib");
    assert!(
        runtime_vm_lib.contains("pub mod adapter_world_interface;"),
        "{runtime_vm_lib}"
    );
    assert_eq!(workspace_lib, "pub mod workspace_only;\n");
    assert!(
        root.join("crates/runtime/runtime_vm/src/adapter_world_interface.rs")
            .exists()
    );
}
