#[test]
fn no_legacy_route_helpers_remain() {
    let repl = include_str!("../../src/repl.rs");
    let composer = include_str!("../../src/tui/composer.rs");
    let session = include_str!("../../src/nl/session.rs");
    let planner = include_str!("../../src/nl/planner_v2.rs");
    let executor = include_str!("../../src/nl/executor.rs");
    let nl_mod = include_str!("../../src/nl/mod.rs");

    let blocked = [
        ["/coding", " rollback"].concat(),
        ["undo previous", " transaction"].concat(),
        ["com", "pat"].concat(),
        ["sh", "im"].concat(),
    ];

    for source in [repl, composer, session, planner, executor, nl_mod] {
        for needle in &blocked {
            assert!(!source.contains(needle), "unexpected token: {needle}");
        }
    }
}

#[test]
fn planner_precedence_tree_is_ir_only() {
    let planner = include_str!("../../src/nl/planner_v2.rs");
    assert!(planner.contains("PlannedStep::RollbackCurrentTransaction"));
    assert!(planner.contains("is_explicit_apply_intent"));
    assert!(!planner.contains("/coding rollback"));
    assert!(!planner.contains("undo previous transaction"));
}

#[test]
fn rollback_executor_has_no_compat_shim() {
    let executor = include_str!("../../src/nl/executor.rs");
    assert!(executor.contains("execute_ir_rollback"));
    assert!(executor.contains("rollback_current_transaction"));
    assert!(!executor.contains(&["com", "pat"].concat()));
    assert!(!executor.contains(&["sh", "im"].concat()));
}

#[test]
fn dto_has_no_legacy_or_compat_field_names() {
    let dto = include_str!("../../src/service/dto.rs");
    let service = include_str!("../../src/service.rs");
    let app = include_str!("../../src/app.rs");
    for source in [dto, service, app] {
        assert!(!source.contains("legacy"));
        assert!(!source.contains("compat"));
        assert!(!source.contains("fallback"));
        assert!(!source.contains("deprecated"));
    }
}

#[test]
fn telemetry_uses_canonical_ir_labels_only() {
    let coding = include_str!("../../src/coding.rs");
    assert!(coding.contains("CanonicalizationTelemetry"));
    assert!(coding.contains("resolution_pipeline_hits"));
    assert!(coding.contains("degraded_resolution_hits"));
    assert!(!coding.contains("LegacyLoweringTelemetry"));
    assert!(!coding.contains("legacy_pipeline_hits"));
    assert!(!coding.contains("fallback_resolution_hits"));
}

#[test]
fn canonical_target_dto_is_single_authority() {
    let dto = include_str!("../../src/service/dto.rs");
    let coding = include_str!("../../src/coding.rs");
    assert!(coding.contains("canonical_target"));
    assert!(coding.contains("canonical_target_path"));
    assert!(!dto.contains("canonical_target_path"));
}
