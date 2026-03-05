use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use agent_core::domain::ProposedDiff;
use agent_core::domain::hash::compute_hash;
use design_gui::app::{DesignApp, GuiEvent, handle_event};
use design_gui::persistence::{
    MAX_DELTAS, PERSISTED_SCHEMA_VERSION, PersistError, PersistedState, app_state_from_persisted,
    load_checkpoint, load_checkpoint_at_version, load_checkpoint_history, save_checkpoint,
};

fn unique_test_dir(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("{prefix}_{nanos}"));
    fs::create_dir_all(&dir).expect("create temp test dir");
    dir
}

#[test]
fn save_checkpoint_writes_expected_file() {
    let dir = unique_test_dir("dbm_save_ok");
    let path = dir.join("project.dbm");

    let app = DesignApp::new_with_checkpoint_path(path.clone());
    handle_event(
        GuiEvent::ApplyDiff(ProposedDiff::UpsertNode {
            key: "a".to_string(),
            value: "alpha".to_string(),
        }),
        &app.domain_state,
    )
    .expect("diff apply should succeed");

    let snapshot = app.domain_state.read().expect("read lock").clone();
    save_checkpoint(&snapshot, &path).expect("save should succeed");

    assert!(path.exists());
    let loaded = load_checkpoint(&path)
        .expect("load should succeed")
        .expect("checkpoint should exist");
    assert_eq!(loaded.uds.nodes.get("a"), Some(&"alpha".to_string()));
}

#[test]
fn save_failure_keeps_existing_checkpoint_unchanged() {
    let dir = unique_test_dir("dbm_save_fail");
    let path = dir.join("project.dbm");

    let original = PersistedState {
        schema_version: PERSISTED_SCHEMA_VERSION,
        version_id: 1,
        uds_hash: 0,
        uds: Default::default(),
        evaluation: Default::default(),
        timestamp: 100,
        metadata: None,
    };
    let original_bytes = serde_json::to_vec_pretty(&original).expect("serialize original");
    fs::write(&path, &original_bytes).expect("write original file");

    // Force tmp file creation to fail by making `<path>.tmp` a directory.
    let tmp_path = path.with_extension("tmp");
    fs::create_dir_all(&tmp_path).expect("create tmp directory collision");

    let app = DesignApp::new_with_checkpoint_path(path.clone());
    let snapshot = app.domain_state.read().expect("read lock").clone();
    let err = save_checkpoint(&snapshot, &path).expect_err("save should fail");
    let _ = err;

    let after = fs::read(&path).expect("read file after failed save");
    assert_eq!(after, original_bytes);
}

#[test]
fn load_restores_hash_and_evaluation() {
    let dir = unique_test_dir("dbm_load");
    let path = dir.join("project.dbm");

    let app = DesignApp::new_with_checkpoint_path(path.clone());
    handle_event(
        GuiEvent::ApplyDiff(ProposedDiff::UpsertNode {
            key: "n1".to_string(),
            value: "v1".to_string(),
        }),
        &app.domain_state,
    )
    .expect("apply diff should succeed");

    let saved_snapshot = app.domain_state.read().expect("read lock").clone();
    let saved_hash = compute_hash(&saved_snapshot.uds);
    let saved_eval = saved_snapshot.evaluation.clone();

    save_checkpoint(&saved_snapshot, &path).expect("save should succeed");

    let loaded = load_checkpoint(&path)
        .expect("load should succeed")
        .expect("checkpoint exists");
    let restored = app_state_from_persisted(loaded);

    assert_eq!(compute_hash(&restored.uds), saved_hash);
    assert_eq!(restored.evaluation, saved_eval);
    assert_eq!(restored.session_history.len(), 1);
}

#[test]
fn checkpoint_file_is_unchanged_after_undo() {
    let dir = unique_test_dir("dbm_undo_file_static");
    let path = dir.join("project.dbm");

    let app = DesignApp::new_with_checkpoint_path(path.clone());
    handle_event(
        GuiEvent::ApplyDiff(ProposedDiff::UpsertNode {
            key: "n1".to_string(),
            value: "v1".to_string(),
        }),
        &app.domain_state,
    )
    .expect("apply diff should succeed");

    let snapshot = app.domain_state.read().expect("read lock").clone();
    save_checkpoint(&snapshot, &path).expect("save should succeed");
    let before = fs::read(&path).expect("read checkpoint");

    handle_event(GuiEvent::Undo, &app.domain_state).expect("undo should succeed");
    let after = fs::read(&path).expect("read checkpoint after undo");

    assert_eq!(before, after);
}

#[test]
fn save_then_mutate_state_keeps_checkpoint_snapshot_immutable() {
    let dir = unique_test_dir("dbm_deep_clone");
    let path = dir.join("project.dbm");

    let app = DesignApp::new_with_checkpoint_path(path.clone());
    handle_event(
        GuiEvent::ApplyDiff(ProposedDiff::UpsertNode {
            key: "fixed".to_string(),
            value: "before-save".to_string(),
        }),
        &app.domain_state,
    )
    .expect("apply diff should succeed");

    let saved = app.domain_state.read().expect("read lock").clone();
    let saved_hash = compute_hash(&saved.uds);
    save_checkpoint(&saved, &path).expect("save should succeed");

    handle_event(
        GuiEvent::ApplyDiff(ProposedDiff::UpsertNode {
            key: "fixed".to_string(),
            value: "after-save".to_string(),
        }),
        &app.domain_state,
    )
    .expect("mutating state after save should succeed");

    let loaded = load_checkpoint(&path)
        .expect("load should succeed")
        .expect("checkpoint should exist");
    assert_eq!(
        loaded.uds.nodes.get("fixed"),
        Some(&"before-save".to_string())
    );
    assert_eq!(compute_hash(&loaded.uds), saved_hash);
}

#[test]
fn tampered_hash_is_detected_as_integrity_violation() {
    let dir = unique_test_dir("dbm_tamper_hash");
    let path = dir.join("project.dbm");

    let app = DesignApp::new_with_checkpoint_path(path.clone());
    let snapshot = app.domain_state.read().expect("read lock").clone();
    save_checkpoint(&snapshot, &path).expect("save should succeed");

    let mut v: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("read saved file")).expect("json parse");
    let current = v["base"]["uds_hash"].as_u64().expect("uds_hash as u64");
    v["base"]["uds_hash"] = serde_json::Value::from(current.saturating_add(1));
    fs::write(
        &path,
        serde_json::to_vec_pretty(&v).expect("serialize tampered json"),
    )
    .expect("write tampered file");

    let err = load_checkpoint(&path).expect_err("tampered hash must fail");
    assert!(matches!(err, PersistError::IntegrityViolation(_)));
}

#[test]
fn tampered_evaluation_is_detected_as_integrity_violation() {
    let dir = unique_test_dir("dbm_tamper_eval");
    let path = dir.join("project.dbm");

    let app = DesignApp::new_with_checkpoint_path(path.clone());
    let snapshot = app.domain_state.read().expect("read lock").clone();
    save_checkpoint(&snapshot, &path).expect("save should succeed");

    let mut v: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("read saved file")).expect("json parse");
    v["base"]["evaluation"]["consistency"] = serde_json::Value::from(0_u64);
    fs::write(
        &path,
        serde_json::to_vec_pretty(&v).expect("serialize tampered json"),
    )
    .expect("write tampered file");

    let err = load_checkpoint(&path).expect_err("tampered evaluation must fail");
    assert!(matches!(err, PersistError::IntegrityViolation(_)));
}

#[test]
fn dbm_file_is_preferred_when_tmp_residue_exists() {
    let dir = unique_test_dir("dbm_tmp_residue");
    let path = dir.join("project.dbm");
    let tmp_path = path.with_extension("tmp");

    let app = DesignApp::new_with_checkpoint_path(path.clone());
    handle_event(
        GuiEvent::ApplyDiff(ProposedDiff::UpsertNode {
            key: "from_dbm".to_string(),
            value: "stable".to_string(),
        }),
        &app.domain_state,
    )
    .expect("apply diff should succeed");
    let snapshot = app.domain_state.read().expect("read lock").clone();
    save_checkpoint(&snapshot, &path).expect("save dbm");

    let mut tmp_corrupt = serde_json::json!({
        "schema_version": PERSISTED_SCHEMA_VERSION,
        "version_id": 999,
        "uds_hash": 0,
        "uds": {"nodes": {"from_tmp": "unstable"}, "dependencies": {}},
        "evaluation": {
            "consistency": 0,
            "structural_integrity": 0,
            "dependency_soundness": 0
        },
        "timestamp": 0
    });
    // keep tmp syntactically valid but content invalid; loader must ignore tmp anyway.
    tmp_corrupt["uds_hash"] = serde_json::Value::from(12345_u64);
    fs::write(
        &tmp_path,
        serde_json::to_vec_pretty(&tmp_corrupt).expect("serialize tmp residue"),
    )
    .expect("write tmp residue");

    let loaded = load_checkpoint(&path)
        .expect("load should read dbm")
        .expect("checkpoint exists");
    assert_eq!(
        loaded.uds.nodes.get("from_dbm"),
        Some(&"stable".to_string())
    );
    assert!(!loaded.uds.nodes.contains_key("from_tmp"));
}

#[test]
fn corrupted_json_falls_back_to_default_state_on_startup() {
    let dir = unique_test_dir("dbm_corrupt_json");
    let path = dir.join("project.dbm");
    fs::write(&path, b"{ this is not valid json").expect("write corrupted json");

    let app = DesignApp::new_with_checkpoint_path(path);
    let guard = app.domain_state.read().expect("read lock");
    assert!(guard.uds.nodes.is_empty());
    assert_eq!(guard.session_history.len(), 1);
}

#[test]
fn save_load_then_analyze_keeps_hash_and_history_stable() {
    let dir = unique_test_dir("dbm_save_load_analyze");
    let path = dir.join("project.dbm");

    let app = DesignApp::new_with_checkpoint_path(path.clone());
    handle_event(
        GuiEvent::ApplyDiff(ProposedDiff::UpsertNode {
            key: "n1".to_string(),
            value: "".to_string(),
        }),
        &app.domain_state,
    )
    .expect("apply diff should succeed");

    let snapshot = app.domain_state.read().expect("read lock").clone();
    save_checkpoint(&snapshot, &path).expect("save should succeed");
    let persisted = load_checkpoint(&path)
        .expect("load should succeed")
        .expect("checkpoint exists");
    let mut restored = app_state_from_persisted(persisted);

    let hash_before = compute_hash(&restored.uds);
    let history_before = restored.session_history.len();
    restored.evaluation.consistency = 999;
    restored.evaluate_now().expect("analyze should succeed");

    assert_eq!(compute_hash(&restored.uds), hash_before);
    assert_eq!(restored.session_history.len(), history_before);
    assert_ne!(restored.evaluation.consistency, 999);
}

#[test]
fn load_accepts_legacy_payload_without_metadata_field() {
    let dir = unique_test_dir("dbm_legacy_metadata");
    let path = dir.join("project.dbm");

    let mut legacy_uds = agent_core::domain::UnifiedDesignState::default();
    legacy_uds
        .nodes
        .insert("a".to_string(), "alpha".to_string());
    let legacy_hash = compute_hash(&legacy_uds);

    let legacy_payload = serde_json::json!({
        "schema_version": PERSISTED_SCHEMA_VERSION,
        "version_id": 0,
        "uds_hash": legacy_hash,
        "uds": {
            "nodes": {"a": "alpha"},
            "dependencies": {}
        },
        "evaluation": {
            "consistency": 100,
            "structural_integrity": 100,
            "dependency_soundness": 100
        },
        "timestamp": 1
    });
    fs::write(
        &path,
        serde_json::to_vec_pretty(&legacy_payload).expect("serialize legacy payload"),
    )
    .expect("write legacy payload");

    let loaded = load_checkpoint(&path)
        .expect("legacy load should succeed")
        .expect("checkpoint exists");
    assert!(loaded.metadata.is_none());
    assert_eq!(loaded.uds.nodes.get("a"), Some(&"alpha".to_string()));
}

#[test]
fn save_load_then_suggest_keeps_persistence_consistent() {
    let dir = unique_test_dir("dbm_save_load_suggest");
    let path = dir.join("project.dbm");

    let mut app = DesignApp::new_with_checkpoint_path(path.clone());
    app.view_state.editor_buffer = "node:a=\nnode:b=beta\ndep:a->b".to_string();
    app.sync_editor_to_domain();
    app.trigger_analyze();
    app.trigger_suggest();
    assert!(!app.view_state.suggested_diffs.is_empty());

    let snapshot = app.domain_state.read().expect("read lock").clone();
    let saved_hash = compute_hash(&snapshot.uds);
    save_checkpoint(&snapshot, &path).expect("save should succeed");

    load_checkpoint(&path)
        .expect("load should succeed")
        .expect("checkpoint exists");
    let mut restored_app = DesignApp::new_with_checkpoint_path(path.clone());
    restored_app.trigger_analyze();
    restored_app.trigger_suggest();
    assert!(!restored_app.view_state.suggested_diffs.is_empty());

    let restored_hash = {
        let s = restored_app.domain_state.read().expect("read lock");
        compute_hash(&s.uds)
    };
    assert_eq!(restored_hash, saved_hash);
}

#[test]
fn base_plus_three_deltas_restore_consistent() {
    let dir = unique_test_dir("dbm_history_restore");
    let path = dir.join("project.dbm");
    let app = DesignApp::new_with_checkpoint_path(path.clone());

    let snapshot0 = app.domain_state.read().expect("read lock").clone();
    save_checkpoint(&snapshot0, &path).expect("save base");

    for i in 1..=3 {
        handle_event(
            GuiEvent::ApplyDiff(ProposedDiff::UpsertNode {
                key: format!("k{i}"),
                value: format!("v{i}"),
            }),
            &app.domain_state,
        )
        .expect("apply diff");
        let snapshot = app.domain_state.read().expect("read lock").clone();
        save_checkpoint(&snapshot, &path).expect("save delta");
    }

    let history = load_checkpoint_history(&path)
        .expect("load history")
        .expect("history exists");
    assert_eq!(history.deltas.len(), 3);

    let latest = load_checkpoint(&path)
        .expect("load checkpoint")
        .expect("latest exists");
    let current_hash = {
        let s = app.domain_state.read().expect("read lock");
        compute_hash(&s.uds)
    };
    assert_eq!(latest.uds_hash, current_hash);
}

#[test]
fn tampered_delta_is_detected() {
    let dir = unique_test_dir("dbm_history_tamper_delta");
    let path = dir.join("project.dbm");
    let app = DesignApp::new_with_checkpoint_path(path.clone());

    let base = app.domain_state.read().expect("read lock").clone();
    save_checkpoint(&base, &path).expect("save base");
    handle_event(
        GuiEvent::ApplyDiff(ProposedDiff::UpsertNode {
            key: "k".to_string(),
            value: "v".to_string(),
        }),
        &app.domain_state,
    )
    .expect("apply diff");
    let snapshot = app.domain_state.read().expect("read lock").clone();
    save_checkpoint(&snapshot, &path).expect("save delta");

    let mut v: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("read file")).expect("parse json");
    let current = v["deltas"][0]["resulting_hash"]
        .as_u64()
        .expect("delta hash");
    v["deltas"][0]["resulting_hash"] = serde_json::Value::from(current.saturating_add(1));
    fs::write(
        &path,
        serde_json::to_vec_pretty(&v).expect("serialize tampered"),
    )
    .expect("write tampered");

    let err = load_checkpoint(&path).expect_err("tampered delta must fail");
    assert!(matches!(err, PersistError::IntegrityViolation(_)));
}

#[test]
fn tampered_base_is_detected() {
    let dir = unique_test_dir("dbm_history_tamper_base");
    let path = dir.join("project.dbm");
    let app = DesignApp::new_with_checkpoint_path(path.clone());

    let snapshot = app.domain_state.read().expect("read lock").clone();
    save_checkpoint(&snapshot, &path).expect("save base");

    let mut v: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("read file")).expect("parse json");
    let current = v["base"]["uds_hash"].as_u64().expect("base hash");
    v["base"]["uds_hash"] = serde_json::Value::from(current.saturating_add(1));
    fs::write(
        &path,
        serde_json::to_vec_pretty(&v).expect("serialize tampered"),
    )
    .expect("write tampered");

    let err = load_checkpoint(&path).expect_err("tampered base must fail");
    assert!(matches!(err, PersistError::IntegrityViolation(_)));
}

#[test]
fn max_deltas_threshold_triggers_rebase() {
    let dir = unique_test_dir("dbm_history_rebase");
    let path = dir.join("project.dbm");
    let app = DesignApp::new_with_checkpoint_path(path.clone());

    let snapshot0 = app.domain_state.read().expect("read lock").clone();
    save_checkpoint(&snapshot0, &path).expect("save base");

    for i in 1..=(MAX_DELTAS as u64 + 2) {
        handle_event(
            GuiEvent::ApplyDiff(ProposedDiff::UpsertNode {
                key: format!("node-{i}"),
                value: format!("value-{i}"),
            }),
            &app.domain_state,
        )
        .expect("apply diff");
        let snap = app.domain_state.read().expect("read lock").clone();
        save_checkpoint(&snap, &path).expect("save");
    }

    let history = load_checkpoint_history(&path)
        .expect("load history")
        .expect("history exists");
    assert!(history.deltas.len() <= MAX_DELTAS);
}

#[test]
fn restore_from_history_starts_with_empty_undo_stack() {
    let dir = unique_test_dir("dbm_history_restore_undo");
    let path = dir.join("project.dbm");
    let app = DesignApp::new_with_checkpoint_path(path.clone());

    let snapshot0 = app.domain_state.read().expect("read lock").clone();
    save_checkpoint(&snapshot0, &path).expect("save base");

    handle_event(
        GuiEvent::ApplyDiff(ProposedDiff::UpsertNode {
            key: "k1".to_string(),
            value: "v1".to_string(),
        }),
        &app.domain_state,
    )
    .expect("apply diff");
    let snapshot1 = app.domain_state.read().expect("read lock").clone();
    let target_version = snapshot1.current_version_id();
    save_checkpoint(&snapshot1, &path).expect("save delta");

    let restored = load_checkpoint_at_version(&path, target_version)
        .expect("load at version")
        .expect("version should exist");
    assert_eq!(restored.session_history.len(), 1);
}
