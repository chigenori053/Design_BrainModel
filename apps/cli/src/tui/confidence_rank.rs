use crate::tui::edit_block::EditBlock;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConfidenceRank {
    pub score: f32,
    pub label: &'static str,
    pub risk_label: &'static str,
    pub destructive: bool,
    pub cross_crate: bool,
}

pub fn rank_edit_block(file_path: &str, operation: &str, replacement: &str) -> ConfidenceRank {
    let destructive = operation == "delete"
        || replacement.contains("pub ")
        || file_path.ends_with("lib.rs")
        || file_path.ends_with("mod.rs");
    let cross_crate = file_path.starts_with("crates/");
    let score = if destructive {
        0.22
    } else if cross_crate {
        0.41
    } else if file_path.starts_with("apps/") {
        0.68
    } else {
        0.84
    };
    let label = if score < 0.34 {
        "low"
    } else if score < 0.67 {
        "medium"
    } else {
        "high"
    };
    let risk_label = match label {
        "low" => "high",
        "medium" => "medium",
        _ => "low",
    };
    ConfidenceRank {
        score,
        label,
        risk_label,
        destructive,
        cross_crate,
    }
}

pub fn sort_edit_blocks(blocks: &mut [EditBlock]) {
    blocks.sort_by(|left, right| {
        rank_key(left)
            .cmp(&rank_key(right))
            .then(left.patch_id.cmp(&right.patch_id))
    });
}

fn rank_key(block: &EditBlock) -> (u8, u8, u8, String) {
    (
        confidence_order(block.confidence_label.as_str()),
        destructive_order(block.destructive),
        cross_crate_order(block.cross_crate),
        block.patch_id.clone(),
    )
}

fn confidence_order(label: &str) -> u8 {
    match label {
        "low" => 0,
        "medium" => 1,
        _ => 2,
    }
}

fn destructive_order(destructive: bool) -> u8 {
    if destructive { 0 } else { 1 }
}

fn cross_crate_order(cross_crate: bool) -> u8 {
    if cross_crate { 0 } else { 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::edit_block::{EditBlock, EditBlockStatus};

    #[test]
    fn confidence_rank_orders_low_confidence_first() {
        let mut blocks = vec![
            sample_block("apps/cli/src/app.rs", "modify", "fn x() {}", "patch-b"),
            sample_block(
                "crates/core/src/lib.rs",
                "modify",
                "pub fn x() {}",
                "patch-a",
            ),
            sample_block("apps/cli/src/repl.rs", "modify", "fn y() {}", "patch-c"),
        ];
        sort_edit_blocks(&mut blocks);
        assert_eq!(blocks[0].patch_id, "patch-a");
    }

    #[test]
    fn destructive_edits_precede_safe_edits() {
        let mut blocks = vec![
            sample_block("crates/core/src/lib.rs", "modify", "fn x() {}", "patch-b"),
            sample_block(
                "crates/core/src/lib.rs",
                "delete",
                "pub fn y() {}",
                "patch-a",
            ),
        ];
        sort_edit_blocks(&mut blocks);
        assert_eq!(blocks[0].patch_id, "patch-a");
    }

    fn sample_block(
        file_path: &str,
        operation: &str,
        replacement: &str,
        patch_id: &str,
    ) -> EditBlock {
        let rank = rank_edit_block(file_path, operation, replacement);
        EditBlock {
            patch_id: patch_id.to_string(),
            file_path: file_path.to_string(),
            confidence_label: rank.label.to_string(),
            confidence_score: rank.score,
            risk_label: rank.risk_label.to_string(),
            hunk_count: 1,
            operation: operation.to_string(),
            diff_lines: vec![],
            replacement: replacement.to_string(),
            added_lines: 0,
            removed_lines: 0,
            expanded: false,
            status: EditBlockStatus::Pending,
            selected_for_batch: false,
            patch_family: "family".to_string(),
            crate_name: "core".to_string(),
            target_directory: "src".to_string(),
            destructive: rank.destructive,
            cross_crate: rank.cross_crate,
        }
    }
}
