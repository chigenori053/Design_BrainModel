use design_cli::core::{CoreExecutor, CoreRequest, RuntimeCoreBridge};

fn dump_result(label: &str, result: &design_cli::core::CoreResponse) {
    println!("{label}.status={:?}", result.status);
    println!("{label}.events={:#?}", result.events);
}

// CATEGORY: INTEGRATION
#[test]
fn test_policy_layer_enforcement() {
    let core = RuntimeCoreBridge::with_defaults();

    // ── Case 1: Reviewer cannot Modify ──
    core.execute(CoreRequest::new("査読者として実行してください".to_string()));
    let res1 = core.execute(CoreRequest::new(
        "apps/cli/src/core.rs に TEST コメントを追加してください".to_string(),
    ));
    dump_result("policy.case1_reviewer_modify", &res1);
    assert!(res1.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("PermissionDenied"))));
    assert!(res1.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Debug { message } if message.contains("[POLICY_EVALUATION]"))));

    // ── Case 2: Developer can Modify (Planning) ──
    core.execute(CoreRequest::new("開発者として実行してください".to_string()));
    let res2 = core.execute(CoreRequest::new(
        "apps/cli/src/core.rs に TEST コメントを追加してください".to_string(),
    ));
    dump_result("policy.case2_developer_modify", &res2);
    assert!(!res2.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("PermissionDenied"))));
    assert!(res2.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Debug { message } if message.contains("[EXECUTION] status=Planning"))));

    // ── Case 3: Reviewer cannot Apply after Apply prerequisites exist ──
    let core = RuntimeCoreBridge::with_defaults();
    core.execute(CoreRequest::new("開発者として実行してください".to_string()));
    let plan = core.execute(CoreRequest::new(
        "apps/cli/src/core.rs に TEST コメントを追加してください".to_string(),
    ));
    dump_result("policy.case3_plan", &plan);
    assert!(plan.events.iter().any(
        |e| matches!(e, design_cli::core::CoreEvent::Pipeline { state } if state == "Proposed")
    ));

    core.execute(CoreRequest::new("査読者として実行してください".to_string()));
    let preview = core.execute(CoreRequest::new("select 1".to_string()));
    dump_result("policy.case3_preview", &preview);
    assert!(preview.events.iter().any(
        |e| matches!(e, design_cli::core::CoreEvent::Pipeline { state } if state == "Previewed")
    ));

    let validate = core.execute(CoreRequest::new("validate selected plan".to_string()));
    dump_result("policy.case3_validate", &validate);
    assert!(validate.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Debug { message } if message.contains("[IR-TRACE][PLAN_VALIDATION]"))));

    let res3 = core.execute(CoreRequest::new("/apply".to_string()));
    dump_result("policy.case3_reviewer_apply", &res3);
    assert!(res3.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("PermissionDenied"))));

    // ── Case 4: Operator can Apply ──
    let core = RuntimeCoreBridge::with_defaults();
    core.execute(CoreRequest::new("運用者として実行してください".to_string()));
    // ...

    // ── Case 5: Reviewer can Analyze ──
    core.execute(CoreRequest::new("査読者として実行してください".to_string()));
    let res5 = core.execute(CoreRequest::new(
        "このプロジェクト全体の構造を解析してください".to_string(),
    ));
    dump_result("policy.case5_reviewer_analyze", &res5);
    assert!(!res5.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("PermissionDenied"))));

    // ── Case 6: Role Switch via 'mode' ──
    let core = RuntimeCoreBridge::with_defaults();
    let res6 = core.execute(CoreRequest::new("開発者モードにしてください".to_string()));
    dump_result("policy.case6_developer_mode", &res6);
    assert!(res6.status == design_cli::core::ExecutionStatus::Executed);
    assert!(res6.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Debug { message } if message.contains("role=Developer"))));

    let res7 = core.execute(CoreRequest::new("運用者モードにしてください".to_string()));
    dump_result("policy.case6_operator_mode", &res7);
    assert!(res7.status == design_cli::core::ExecutionStatus::Executed);
    assert!(res7.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Debug { message } if message.contains("role=Operator"))));
}
