use design_cli::core::{CoreExecutor, CoreRequest, RuntimeCoreBridge};

fn dump_result(label: &str, result: &design_cli::core::CoreResponse) {
    println!("{label}.status={:?}", result.status);
    println!("{label}.events={:#?}", result.events);
}

// CATEGORY: INTEGRATION
#[test]
fn test_constraint_enforcement() {
    let core = RuntimeCoreBridge::with_defaults();

    // ── Case 1 & 2: NoApply ──
    core.execute(CoreRequest::new("まだ適用しないでください".to_string()));
    let res1 = core.execute(CoreRequest::new(
        "apps/cli/src/core.rs に TEST コメントを追加してください".to_string(),
    ));
    dump_result("constraint.case1_no_apply", &res1);
    assert!(res1.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("NoApplyConstraint"))));

    let res2 = core.execute(CoreRequest::new(
        "このプロジェクト全体の構造を解析してください".to_string(),
    ));
    dump_result("constraint.case2_analyze_allowed", &res2);
    assert!(!res2.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("NoApplyConstraint"))));

    // Reset session (new core)
    let core = RuntimeCoreBridge::with_defaults();

    // ── Case 3: NoGit ──
    // Current runtime evaluates Policy before Constraint. The session NoGit
    // constraint removes Git permission from the active policy profile, so a
    // direct git command is rejected at the policy layer before reaching
    // ConstraintEvaluator::evaluate_git.
    core.execute(CoreRequest::new("git操作しないでください".to_string()));
    let res3 = core.execute(CoreRequest::new("git status".to_string()));
    dump_result("constraint.case3_no_git", &res3);
    assert!(res3.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("PermissionDenied"))));

    // Reset session
    let core = RuntimeCoreBridge::with_defaults();

    // ── Case 4: NoExternalCommand ──
    // As with NoGit, policy is evaluated first after constraints are applied
    // to the active policy profile.
    core.execute(CoreRequest::new(
        "外部コマンド実行しないでください".to_string(),
    ));
    let res4 = core.execute(CoreRequest::new("cargo test".to_string()));
    dump_result("constraint.case4_no_external_command", &res4);
    assert!(res4.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("PermissionDenied"))));

    // Reset session
    let core = RuntimeCoreBridge::with_defaults();

    // ── Case 5: NoDelete ──
    // Delete permission is also removed from the active policy profile before
    // the constraint evaluator runs.
    core.execute(CoreRequest::new("まだ削除しないでください".to_string()));
    let res5 = core.execute(CoreRequest::new("README.md を削除してください".to_string()));
    dump_result("constraint.case5_no_delete", &res5);
    assert!(res5.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("PermissionDenied"))));

    // Reset session
    let core = RuntimeCoreBridge::with_defaults();

    // ── Case 6: No Constraint ──
    let res6 = core.execute(CoreRequest::new(
        "apps/cli/src/core.rs に TEST コメントを追加してください".to_string(),
    ));
    dump_result("constraint.case6_no_constraint", &res6);
    // Error indicates clarification or something else, but NOT NoApplyConstraint
    assert!(!res6.events.iter().any(|e| matches!(e, design_cli::core::CoreEvent::Error { message } if message.contains("NoApplyConstraint"))));
}
