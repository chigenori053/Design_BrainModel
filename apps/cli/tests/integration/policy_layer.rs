use design_cli::core::{CoreExecutor, CoreRequest, RuntimeCoreBridge};

#[test]
fn test_policy_layer_enforcement() {
    let core = RuntimeCoreBridge::with_defaults();

    // ── Case 1: Reviewer cannot Modify ──
    core.execute(CoreRequest::new("査読者として実行してください".to_string()));
    let res1 = core.execute(CoreRequest::new("apps/cli/src/core.rs に TEST コメントを追加してください".to_string()));
    assert!(res1.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("PermissionDenied"))));
    assert!(res1.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Debug { message } if message.contains("[POLICY_EVALUATION]"))));

    // ── Case 2: Developer can Modify (Planning) ──
    core.execute(CoreRequest::new("開発者として実行してください".to_string()));
    let res2 = core.execute(CoreRequest::new("apps/cli/src/core.rs に TEST コメントを追加してください".to_string()));
    assert!(!res2.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("PermissionDenied"))));
    assert!(res2.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Debug { message } if message.contains("[EXECUTION] status=Planning"))));

    // ── Case 3: Developer cannot Apply ──
    let _res3 = core.execute(CoreRequest::new("apply".to_string()));
    assert!(_res3.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("PermissionDenied"))));
    
    // ── Case 4: Operator can Apply ──
    core.execute(CoreRequest::new("運用者として実行してください".to_string()));
    // ...
    
    // ── Case 5: Reviewer can Analyze ──
    core.execute(CoreRequest::new("査読者として実行してください".to_string()));
    let res5 = core.execute(CoreRequest::new("このプロジェクト全体の構造を解析してください".to_string()));
    assert!(!res5.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("PermissionDenied"))));

    // ── Case 6: Role Switch via 'mode' ──
    let res6 = core.execute(CoreRequest::new("開発者モードにしてください".to_string()));
    assert!(res6.status == design_cli::core::ExecutionStatus::Executed);
    assert!(res6.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Debug { message } if message.contains("role=Developer"))));

    let res7 = core.execute(CoreRequest::new("運用者モードにしてください".to_string()));
    assert!(res7.status == design_cli::core::ExecutionStatus::Executed);
    assert!(res7.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Debug { message } if message.contains("role=Operator"))));
}
