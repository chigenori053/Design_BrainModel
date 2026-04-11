use std::fs;
use std::path::{Path, PathBuf};

use design_cli::nl::autonomous::{
    HookTelemetry, KpiSnapshot, write_nightly_optimization_report, write_origin_benchmark_snapshot,
    write_regression_scorecard,
};
use design_cli::nl::r#loop::LoopOrigin;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

#[test]
fn origin_snapshots_written() {
    let records = vec![
        HookTelemetry {
            origin: LoopOrigin::Analyze,
            promoted: true,
            converged: true,
            retries: 0,
            false_promotion: false,
            rollback_used: false,
        },
        HookTelemetry {
            origin: LoopOrigin::MemoryRecall,
            promoted: true,
            converged: true,
            retries: 0,
            false_promotion: false,
            rollback_used: false,
        },
    ];
    let snapshot = write_origin_benchmark_snapshot(&records).expect("snapshot should write");
    assert_eq!(snapshot.analyze_convergence, 1.0);
    assert!(workspace_root()
        .join(".dbm/benchmarks/origin_benchmark_snapshot.json")
        .exists());
}

#[test]
fn baseline_comparison_and_scorecard_are_generated() {
    let root = workspace_root();
    let telemetry_dir = root.join(".dbm/telemetry");
    fs::create_dir_all(&telemetry_dir).expect("telemetry dir");
    fs::write(
        telemetry_dir.join("phase_e_kpi_snapshot.json"),
        serde_json::to_string_pretty(&KpiSnapshot {
            analyze_convergence: 0.5,
            coding_retry_success: 0.5,
            validate_self_heal: 0.5,
            structure_bind_precision: 0.5,
            memory_false_promotion: 0.1,
        })
        .expect("serialize"),
    )
    .expect("write prior kpi");

    let records = vec![HookTelemetry {
        origin: LoopOrigin::Analyze,
        promoted: true,
        converged: true,
        retries: 1,
        false_promotion: false,
        rollback_used: false,
    }];
    let current_kpi = KpiSnapshot {
        analyze_convergence: 1.0,
        coding_retry_success: 0.0,
        validate_self_heal: 0.0,
        structure_bind_precision: 1.0,
        memory_false_promotion: 0.0,
    };
    let scorecard =
        write_regression_scorecard(11, 4, &current_kpi, &records).expect("scorecard should write");
    assert_eq!(scorecard.hook_sensitive_delta, 0);
    assert!(root.join(".dbm/benchmarks/regression_scorecard.md").exists());
}

#[test]
fn nightly_report_is_written() {
    let records = vec![HookTelemetry {
        origin: LoopOrigin::Analyze,
        promoted: true,
        converged: true,
        retries: 0,
        false_promotion: false,
        rollback_used: false,
    }];
    let report = write_nightly_optimization_report(
        &records,
        &design_cli::nl::autonomous::TunedPolicy {
            analyze_threshold: 0.6,
            memory_threshold: 0.8,
            retry_budget_overrides: std::collections::HashMap::new(),
        },
        &design_cli::nl::autonomous::RegressionScorecard {
            baseline_failed: 12,
            current_failed: 11,
            baseline_hook_sensitive: 4,
            current_hook_sensitive: 4,
            failure_delta: -1,
            hook_sensitive_delta: 0,
            convergence_delta: 0.1,
            retry_median_delta: 0.0,
            false_promotion_delta: -0.1,
        },
    )
    .expect("nightly report should write");
    assert!(report.exists());
}
