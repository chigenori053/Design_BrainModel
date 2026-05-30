use design_cli::core::{CoreExecutor, CoreRequest, RuntimeCoreBridge};

#[test]
fn test_constraint_enforcement() {
    let core = RuntimeCoreBridge::with_defaults();

    // ── Case 1 & 2: NoApply ──
    core.execute(CoreRequest::new("まだ適用しないでください".to_string()));
    let res1 = core.execute(CoreRequest::new("apps/cli/src/core.rs に TEST コメントを追加してください".to_string()));
    assert!(res1.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("NoApplyConstraint"))));

    let res2 = core.execute(CoreRequest::new("このプロジェクト全体の構造を解析してください".to_string()));
    assert!(!res2.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("NoApplyConstraint"))));

    // Reset session (new core)
    let core = RuntimeCoreBridge::with_defaults();
    
    // ── Case 3: NoGit ──
    core.execute(CoreRequest::new("git操作しないでください".to_string()));
    let res3 = core.execute(CoreRequest::new("git status".to_string()));
    assert!(res3.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("NoGitConstraint"))));

    // Reset session
    let core = RuntimeCoreBridge::with_defaults();
    
    // ── Case 4: NoExternalCommand ──
    core.execute(CoreRequest::new("外部コマンド実行しないでください".to_string()));
    let res4 = core.execute(CoreRequest::new("cargo test".to_string()));
    assert!(res4.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("NoExternalCommandConstraint"))));

    // Reset session
    let core = RuntimeCoreBridge::with_defaults();
    
    // ── Case 5: NoDelete ──
    core.execute(CoreRequest::new("まだ削除しないでください".to_string()));
    let res5 = core.execute(CoreRequest::new("README.md を削除してください".to_string()));
    assert!(res5.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("NoDeleteConstraint"))));

    // Reset session
    let core = RuntimeCoreBridge::with_defaults();
    
    // ── Case 6: No Constraint ──
    let res6 = core.execute(CoreRequest::new("apps/cli/src/core.rs に TEST コメントを追加してください".to_string()));
    // Error indicates clarification or something else, but NOT NoApplyConstraint
    assert!(!res6.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("NoApplyConstraint"))));
}
