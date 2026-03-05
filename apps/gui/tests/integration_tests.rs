use agent_core::domain::ProposedDiff;
use agent_core::domain::hash::compute_hash;
use design_gui::app::{DesignApp, GuiEvent, GuiViewState, SharedAppState, handle_event};
use std::path::PathBuf;
use std::thread::sleep;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

struct TestGui {
    domain_state: SharedAppState,
    view_state: GuiViewState,
}

impl TestGui {
    fn new() -> Self {
        let app = DesignApp::new_with_checkpoint_path(unique_test_path("gui_integration"));
        Self {
            domain_state: app.domain_state,
            view_state: app.view_state,
        }
    }

    fn handle(&self, event: GuiEvent) -> Result<(), String> {
        handle_event(event, &self.domain_state)
    }

    fn sync_editor_to_domain(&mut self) {
        let mut app = DesignApp {
            domain_state: self.domain_state.clone(),
            view_state: std::mem::take(&mut self.view_state),
            checkpoint_path: unique_test_path("gui_sync_editor_checkpoint"),
        };
        app.sync_editor_to_domain();
        self.view_state = app.view_state;
    }

    fn history_len(&self) -> usize {
        self.domain_state
            .read()
            .expect("read lock")
            .session_history
            .len()
    }

    fn hash(&self) -> u64 {
        let s = self.domain_state.read().expect("read lock");
        compute_hash(&s.uds)
    }
}

fn unique_test_path(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}_{nanos}.dbm"))
}

#[test]
fn apply_diff_event_commits_single_transaction_and_changes_hash() {
    let gui = TestGui::new();

    let initial_history = gui.history_len();
    let initial_hash = gui.hash();

    gui.handle(GuiEvent::ApplyDiff(ProposedDiff::UpsertNode {
        key: "k1".to_string(),
        value: "v1".to_string(),
    }))
    .expect("apply diff via gui event should succeed");

    let guard = gui.domain_state.read().expect("read lock");
    assert_eq!(guard.session_history.len(), initial_history + 1);
    assert_eq!(guard.uds.nodes.get("k1"), Some(&"v1".to_string()));
    assert_ne!(compute_hash(&guard.uds), initial_hash);
}

#[test]
fn invalid_apply_diff_event_aborts_and_preserves_state() {
    let gui = TestGui::new();

    let initial_history = gui.history_len();
    let initial_hash = gui.hash();

    let err = gui
        .handle(GuiEvent::ApplyDiff(ProposedDiff::SetDependencies {
            key: "missing-node".to_string(),
            dependencies: vec!["x".to_string()],
        }))
        .expect_err("invalid diff should fail");

    assert!(err.contains("apply_diff failed"));
    assert_eq!(gui.history_len(), initial_history);
    assert_eq!(gui.hash(), initial_hash);
}

#[test]
fn editor_sync_updates_uds_and_undo_restores_previous_state() {
    let mut gui = TestGui::new();

    let before_hash = gui.hash();
    gui.view_state.editor_buffer = "node:a=alpha\nnode:b=beta\ndep:a->b".to_string();
    gui.sync_editor_to_domain();

    {
        let guard = gui.domain_state.read().expect("read lock");
        assert_eq!(guard.uds.nodes.get("a"), Some(&"alpha".to_string()));
        assert_eq!(guard.uds.nodes.get("b"), Some(&"beta".to_string()));
        assert_eq!(
            guard.uds.dependencies.get("a"),
            Some(&vec!["b".to_string()])
        );
    }

    gui.handle(GuiEvent::Undo)
        .expect("undo should restore previous snapshot");
    assert_eq!(gui.hash(), before_hash);
}

#[test]
fn undo_redo_via_gui_event_keeps_hash_consistent() {
    let gui = TestGui::new();

    let hash_initial = gui.hash();
    gui.handle(GuiEvent::ApplyDiff(ProposedDiff::UpsertNode {
        key: "n".to_string(),
        value: "1".to_string(),
    }))
    .expect("apply diff should succeed");
    let hash_after_apply = gui.hash();

    gui.handle(GuiEvent::Undo).expect("undo should succeed");
    let hash_after_undo = gui.hash();

    gui.handle(GuiEvent::Redo).expect("redo should succeed");
    let hash_after_redo = gui.hash();

    assert_eq!(hash_after_undo, hash_initial);
    assert_eq!(hash_after_redo, hash_after_apply);
}

#[test]
fn analyze_updates_evaluation_without_changing_uds_hash_or_history() {
    let gui = TestGui::new();

    gui.handle(GuiEvent::ApplyDiff(ProposedDiff::UpsertNode {
        key: "analyze-node".to_string(),
        value: "".to_string(),
    }))
    .expect("apply diff should succeed");

    {
        let mut guard = gui.domain_state.write().expect("write lock");
        guard.evaluation.consistency = 999;
    }

    let uds_before = gui.domain_state.read().expect("read lock").uds.clone();
    let hash_before = gui.hash();
    let history_before = gui.history_len();
    gui.handle(GuiEvent::Analyze)
        .expect("analyze should succeed");

    let guard = gui.domain_state.read().expect("read lock");
    assert_eq!(guard.uds, uds_before);
    assert_eq!(compute_hash(&guard.uds), hash_before);
    assert_eq!(guard.session_history.len(), history_before);
    assert_ne!(guard.evaluation.consistency, 999);
}

#[test]
fn analyze_action_populates_pareto_view_without_history_growth() {
    let mut app = DesignApp::new_with_checkpoint_path(unique_test_path("gui_analyze_view"));
    handle_event(
        GuiEvent::ApplyDiff(ProposedDiff::UpsertNode {
            key: "p".to_string(),
            value: "payload".to_string(),
        }),
        &app.domain_state,
    )
    .expect("apply diff should succeed");

    let history_before = app
        .domain_state
        .read()
        .expect("read lock")
        .session_history
        .len();
    app.trigger_analyze();

    assert!(app.view_state.pareto_result.is_some());
    let history_after = app
        .domain_state
        .read()
        .expect("read lock")
        .session_history
        .len();
    assert_eq!(history_after, history_before);
}

#[test]
fn analyze_metrics_are_recorded_and_timestamp_advances() {
    let mut app = DesignApp::new_with_checkpoint_path(unique_test_path("gui_analyze_metrics"));

    app.trigger_analyze();
    let first = app
        .view_state
        .analyze_metrics
        .clone()
        .expect("first metrics should be available");
    let _ = first.evaluate_duration_ms;
    let _ = first.pareto_duration_ms;
    assert!(first.timestamp > 0);

    sleep(Duration::from_millis(2));
    app.trigger_analyze();
    let second = app
        .view_state
        .analyze_metrics
        .clone()
        .expect("second metrics should be available");
    assert!(second.timestamp > first.timestamp);
}

#[test]
fn suggest_is_visible_and_apply_is_undoable_with_tx_growth() {
    let mut app = DesignApp::new_with_checkpoint_path(unique_test_path("gui_suggest_apply"));
    app.view_state.editor_buffer = "node:a=\nnode:b=beta\ndep:a->b".to_string();
    app.sync_editor_to_domain();

    app.trigger_analyze();
    app.trigger_suggest();
    assert!(!app.view_state.suggested_diffs.is_empty());

    let history_before_apply = app
        .domain_state
        .read()
        .expect("read lock")
        .session_history
        .len();
    let hash_before_apply = {
        let s = app.domain_state.read().expect("read lock");
        compute_hash(&s.uds)
    };

    app.apply_suggested_diff_at(0);
    let history_after_apply = app
        .domain_state
        .read()
        .expect("read lock")
        .session_history
        .len();
    assert_eq!(history_after_apply, history_before_apply + 1);

    handle_event(GuiEvent::Undo, &app.domain_state).expect("undo should succeed");
    let hash_after_undo = {
        let s = app.domain_state.read().expect("read lock");
        compute_hash(&s.uds)
    };
    assert_eq!(hash_after_undo, hash_before_apply);
}

#[test]
fn suggest_is_disabled_when_analyze_metrics_exceed_threshold() {
    let mut app = DesignApp::new_with_checkpoint_path(unique_test_path("gui_suggest_threshold"));
    app.trigger_analyze();
    let mut metrics = app
        .view_state
        .analyze_metrics
        .clone()
        .expect("metrics must exist");
    metrics.evaluate_duration_ms = 999;
    app.view_state.analyze_metrics = Some(metrics);

    app.trigger_suggest();
    assert!(app.view_state.suggested_diffs.is_empty());
    assert!(
        app.view_state
            .error_message
            .as_deref()
            .unwrap_or_default()
            .contains("disabled")
    );
}

#[test]
fn suggest_metrics_counts_accepts_and_rejections() {
    let mut app =
        DesignApp::new_with_checkpoint_path(unique_test_path("gui_suggest_metrics_counts"));
    app.view_state.editor_buffer = "node:a=\nnode:b=beta\ndep:a->a".to_string();
    app.sync_editor_to_domain();
    app.trigger_analyze();
    app.trigger_suggest();

    let metrics = &app.view_state.suggest_metrics;
    assert!(metrics.suggestion_count >= 1);
    assert!(metrics.accepted_count >= 1);
    assert!(metrics.rejected_by_guard >= 1);
}

#[test]
fn suggest_apply_records_gain_only_on_apply_and_survives_undo() {
    let mut app =
        DesignApp::new_with_checkpoint_path(unique_test_path("gui_suggest_apply_metrics"));
    app.view_state.editor_buffer = "node:a=\nnode:b=beta\ndep:a->b".to_string();
    app.sync_editor_to_domain();
    app.trigger_analyze();
    app.trigger_suggest();
    assert!(!app.view_state.suggested_diffs.is_empty());

    let before_apply = app.view_state.suggest_metrics.clone();
    assert_eq!(before_apply.avg_consistency_gain, 0.0);
    assert_eq!(before_apply.avg_structural_gain, 0.0);
    assert_eq!(before_apply.avg_dependency_gain, 0.0);

    app.apply_suggested_diff_at(0);
    let after_apply = app.view_state.suggest_metrics.clone();
    assert!(after_apply.avg_consistency_gain > 0.0);

    handle_event(GuiEvent::Undo, &app.domain_state).expect("undo should succeed");
    let after_undo = app.view_state.suggest_metrics.clone();
    assert_eq!(
        after_undo.avg_consistency_gain,
        after_apply.avg_consistency_gain
    );
}

#[test]
fn gui_module_has_no_direct_domain_bypass_patterns() {
    let source = include_str!("../src/app.rs");

    let forbidden = [
        "domain_state.write().unwrap().uds",
        "domain_state.write().unwrap().evaluation",
        "domain_state.write().unwrap().session_history",
    ];

    for pattern in forbidden {
        assert!(
            !source.contains(pattern),
            "found forbidden direct mutation pattern: {pattern}"
        );
    }

    assert!(source.contains("begin_tx"));
    assert!(source.contains("commit_tx"));
}
