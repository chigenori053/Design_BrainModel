use std::path::{Path, PathBuf};

use design_cli::nl::autonomous::{
    ArchitectureSurgeryScenario, write_architecture_surgery_snapshot,
};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}

#[test]
fn cycle_cut_scenario_is_counted() {
    let snapshot = write_architecture_surgery_snapshot(&[
        ArchitectureSurgeryScenario {
            name: "cycle cut".to_string(),
            compile_pass: true,
            minimal_diff: true,
            rollback_used: false,
            cycle_break_success: true,
        },
        ArchitectureSurgeryScenario {
            name: "unsafe isolation".to_string(),
            compile_pass: true,
            minimal_diff: false,
            rollback_used: true,
            cycle_break_success: false,
        },
    ])
    .expect("architecture surgery snapshot should write");
    assert_eq!(snapshot.compile_pass_rate, 1.0);
    assert_eq!(snapshot.cycle_break_success, 0.5);
}

#[test]
fn trait_extraction_compile_pass_is_recorded() {
    let snapshot = write_architecture_surgery_snapshot(&[ArchitectureSurgeryScenario {
        name: "trait extraction".to_string(),
        compile_pass: true,
        minimal_diff: true,
        rollback_used: false,
        cycle_break_success: false,
    }])
    .expect("trait extraction snapshot should write");
    assert_eq!(snapshot.compile_pass_rate, 1.0);
}

#[test]
fn rollback_metric_is_tracked() {
    write_architecture_surgery_snapshot(&[ArchitectureSurgeryScenario {
        name: "workspace rebinding".to_string(),
        compile_pass: false,
        minimal_diff: false,
        rollback_used: true,
        cycle_break_success: false,
    }])
    .expect("rollback scenario should write");
    assert!(
        workspace_root()
            .join(".dbm/benchmarks/architecture_surgery_snapshot.json")
            .exists()
    );
}
