use std::path::PathBuf;

use design_cli::tui::edit_block::{
    EditBlock, EditBlockStatus, block_visible_lines_for_test, group_header_summary_for_test,
};
use design_cli::tui::review_batch::{ReviewBatchState, ReviewGroup, ReviewGroupingStrategy};

fn sample_block(
    patch_id: &str,
    file_path: &str,
    operation: &str,
    risk: &str,
    confidence: &str,
    hunk_count: usize,
    diff_lines: &[&str],
) -> EditBlock {
    EditBlock {
        patch_id: patch_id.to_string(),
        file_path: file_path.to_string(),
        confidence_label: confidence.to_string(),
        confidence_score: 0.72,
        risk_label: risk.to_string(),
        hunk_count,
        operation: operation.to_string(),
        diff_lines: diff_lines.iter().map(|line| line.to_string()).collect(),
        replacement: "replacement".to_string(),
        expanded: true,
        status: EditBlockStatus::Pending,
        selected_for_batch: false,
        patch_family: "family".to_string(),
        crate_name: "cli".to_string(),
        target_directory: "apps/cli/src".to_string(),
        destructive: false,
        cross_crate: false,
    }
}

#[test]
fn group_header_contains_file_hunk_confidence_risk_and_type() {
    let mut state = ReviewBatchState::empty(PathBuf::from("."));
    state.blocks = vec![sample_block(
        "patch-1",
        "apps/cli/src/repl.rs",
        "modify",
        "medium",
        "high",
        2,
        &["@@ -1,2 +1,2 @@", "-old", "+new"],
    )];
    state.groups = vec![ReviewGroup {
        title: "risk:medium".to_string(),
        block_indices: vec![0],
        selected_count: 0,
        aggregate_risk: "medium".to_string(),
    }];
    state.grouping = ReviewGroupingStrategy::ByRiskLabel;

    let header = group_header_summary_for_test(&state, 0);
    assert!(header.contains("file=apps/cli/src/repl.rs"));
    assert!(header.contains("hunks=2"));
    assert!(header.contains("confidence=high"));
    assert!(header.contains("risk=medium"));
    assert!(header.contains("type=modify"));
}

#[test]
fn diff_lines_preserve_added_removed_and_modified_prefixes() {
    let block = sample_block(
        "patch-2",
        "apps/cli/src/coding.rs",
        "modify",
        "low",
        "medium",
        1,
        &["@@ -3,2 +3,2 @@", "- old", "+ new", " context"],
    );
    let lines = block_visible_lines_for_test(&block);
    assert!(lines.iter().any(|line| line.starts_with("-")));
    assert!(lines.iter().any(|line| line.starts_with("+")));
    assert!(lines.iter().any(|line| line.starts_with("@@")));
}

#[test]
fn file_switching_across_groups_is_deterministic() {
    let mut state = ReviewBatchState::empty(PathBuf::from("."));
    state.blocks = vec![
        sample_block(
            "patch-a",
            "apps/cli/src/a.rs",
            "modify",
            "high",
            "low",
            1,
            &["-before", "+after"],
        ),
        sample_block(
            "patch-b",
            "apps/cli/src/b.rs",
            "create",
            "medium",
            "medium",
            1,
            &["+created"],
        ),
    ];
    state.groups = vec![
        ReviewGroup {
            title: "risk:high".to_string(),
            block_indices: vec![0],
            selected_count: 0,
            aggregate_risk: "high".to_string(),
        },
        ReviewGroup {
            title: "risk:medium".to_string(),
            block_indices: vec![1],
            selected_count: 0,
            aggregate_risk: "medium".to_string(),
        },
    ];

    assert_eq!(state.focused_block, 0);
    state.next_group();
    assert_eq!(state.focused_block, 1);
    state.previous_group();
    assert_eq!(state.focused_block, 0);
}
