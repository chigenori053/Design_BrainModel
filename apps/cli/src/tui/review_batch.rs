use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::coding::apply_code_change_set;
use crate::tui::confidence_rank::sort_edit_blocks;
use crate::tui::edit_block::{
    CodingReviewReport, EditBlock, EditBlockStatus, build_edit_blocks, change_set_for_block,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewGroupingStrategy {
    ByCrate,
    ByTargetDirectory,
    ByRiskLabel,
    ByPatchFamily,
}

impl ReviewGroupingStrategy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ByCrate => "crate",
            Self::ByTargetDirectory => "target_dir",
            Self::ByRiskLabel => "risk",
            Self::ByPatchFamily => "patch_family",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewGroup {
    pub title: String,
    pub block_indices: Vec<usize>,
    pub selected_count: usize,
    pub aggregate_risk: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchEntry {
    pub block_index: usize,
    pub previous_status: EditBlockStatus,
    pub previous_content: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchRecord {
    pub batch_id: String,
    pub entries: Vec<BatchEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReviewBatchState {
    pub root: PathBuf,
    pub blocks: Vec<EditBlock>,
    pub grouping: ReviewGroupingStrategy,
    pub groups: Vec<ReviewGroup>,
    pub current_group: usize,
    pub focused_block: usize,
    pub last_batch: Option<BatchRecord>,
    next_batch_id: usize,
}

impl ReviewBatchState {
    pub fn empty(root: PathBuf) -> Self {
        Self {
            root,
            blocks: Vec::new(),
            grouping: ReviewGroupingStrategy::ByRiskLabel,
            groups: Vec::new(),
            current_group: 0,
            focused_block: 0,
            last_batch: None,
            next_batch_id: 1,
        }
    }

    pub fn from_coding_reports(
        reports: &[(CodingReviewReport, String)],
    ) -> Result<Option<Self>, String> {
        let Some((first, _)) = reports.first() else {
            return Ok(None);
        };
        let root = PathBuf::from(&first.root);
        let mut blocks = Vec::new();
        for (report, family) in reports {
            blocks.extend(build_edit_blocks(report, family)?);
        }
        if blocks.is_empty() {
            return Ok(None);
        }
        sort_edit_blocks(&mut blocks);
        let mut state = Self::empty(root);
        state.blocks = blocks;
        state.refresh_groups();
        Ok(Some(state))
    }

    pub fn refresh_groups(&mut self) {
        self.groups = build_groups(&self.blocks, self.grouping);
        if self.current_group >= self.groups.len() {
            self.current_group = self.groups.len().saturating_sub(1);
        }
        self.focus_current_group();
    }

    pub fn set_grouping(&mut self, grouping: ReviewGroupingStrategy) {
        self.grouping = grouping;
        self.refresh_groups();
    }

    pub fn toggle_expand_focused(&mut self) {
        if let Some(block) = self.blocks.get_mut(self.focused_block) {
            block.expanded = !block.expanded;
        }
    }

    pub fn next_block(&mut self) {
        let Some(group) = self.groups.get(self.current_group) else {
            return;
        };
        if let Some(position) = group
            .block_indices
            .iter()
            .position(|index| *index == self.focused_block)
        {
            let next = (position + 1).min(group.block_indices.len().saturating_sub(1));
            self.focused_block = group.block_indices[next];
        }
    }

    pub fn previous_block(&mut self) {
        let Some(group) = self.groups.get(self.current_group) else {
            return;
        };
        if let Some(position) = group
            .block_indices
            .iter()
            .position(|index| *index == self.focused_block)
        {
            let previous = position.saturating_sub(1);
            self.focused_block = group.block_indices[previous];
        }
    }

    pub fn next_group(&mut self) {
        if self.groups.is_empty() {
            return;
        }
        self.current_group = (self.current_group + 1).min(self.groups.len().saturating_sub(1));
        self.focus_current_group();
    }

    pub fn previous_group(&mut self) {
        self.current_group = self.current_group.saturating_sub(1);
        self.focus_current_group();
    }

    pub fn toggle_batch_selected(&mut self) {
        if let Some(block) = self.blocks.get_mut(self.focused_block)
            && matches!(block.status, EditBlockStatus::Pending)
        {
            block.selected_for_batch = !block.selected_for_batch;
        }
        self.refresh_groups();
    }

    pub fn select_all_in_group(&mut self) -> usize {
        let Some(group) = self.groups.get(self.current_group).cloned() else {
            return 0;
        };
        let mut count = 0;
        for index in group.block_indices {
            if let Some(block) = self.blocks.get_mut(index)
                && matches!(block.status, EditBlockStatus::Pending)
            {
                block.selected_for_batch = true;
                count += 1;
            }
        }
        self.refresh_groups();
        count
    }

    pub fn selected_pending_count(&self) -> usize {
        self.blocks
            .iter()
            .filter(|block| {
                block.selected_for_batch && matches!(block.status, EditBlockStatus::Pending)
            })
            .count()
    }

    pub fn selected_actions_enabled(&self) -> bool {
        self.blocks
            .get(self.focused_block)
            .map(|block| matches!(block.status, EditBlockStatus::Pending))
            .unwrap_or(false)
    }

    pub fn apply_focused_block(&mut self) -> Result<Option<String>, String> {
        let (file_path, hunk_count) = {
            let Some(block) = self.blocks.get_mut(self.focused_block) else {
                return Ok(None);
            };
            if !matches!(block.status, EditBlockStatus::Pending) {
                return Ok(None);
            }
            apply_code_change_set(&self.root, &change_set_for_block(block))?;
            block.status = EditBlockStatus::Applied;
            block.selected_for_batch = false;
            (block.file_path.clone(), block.hunk_count)
        };
        self.refresh_groups();
        Ok(Some(format!(
            "Applied: {} ({} hunks)",
            file_path, hunk_count
        )))
    }

    pub fn discard_focused_block(&mut self) -> Option<String> {
        let block = self.blocks.get_mut(self.focused_block)?;
        if !matches!(block.status, EditBlockStatus::Pending) {
            return None;
        }
        block.status = EditBlockStatus::Discarded;
        block.selected_for_batch = false;
        let summary = format!("Discarded: {}", block.file_path);
        self.refresh_groups();
        Some(summary)
    }

    pub fn apply_selected_batch(&mut self) -> Result<Option<String>, String> {
        let selected = self.selected_block_indices();
        if selected.is_empty() {
            return Ok(None);
        }
        let mut entries = Vec::new();
        for index in &selected {
            let block = &self.blocks[*index];
            let path = self.root.join(&block.file_path);
            entries.push(BatchEntry {
                block_index: *index,
                previous_status: block.status,
                previous_content: fs::read_to_string(&path).ok(),
            });
        }
        for index in &selected {
            let block = &mut self.blocks[*index];
            apply_code_change_set(&self.root, &change_set_for_block(block))?;
            block.status = EditBlockStatus::Applied;
            block.selected_for_batch = false;
        }
        let batch_id = format!("batch-{:04}", self.next_batch_id);
        self.next_batch_id += 1;
        self.last_batch = Some(BatchRecord {
            batch_id: batch_id.clone(),
            entries,
        });
        let title = self
            .groups
            .get(self.current_group)
            .map(|group| group.title.clone())
            .unwrap_or_else(|| "selected".to_string());
        let count = selected.len();
        self.refresh_groups();
        Ok(Some(format!("Applied batch: {title} ({count} files)")))
    }

    pub fn discard_selected_batch(&mut self) -> Option<String> {
        let selected = self.selected_block_indices();
        if selected.is_empty() {
            return None;
        }
        for index in &selected {
            if let Some(block) = self.blocks.get_mut(*index)
                && matches!(block.status, EditBlockStatus::Pending)
            {
                block.status = EditBlockStatus::Discarded;
                block.selected_for_batch = false;
            }
        }
        let count = selected.len();
        self.refresh_groups();
        Some(format!("Discarded selected: {count} blocks"))
    }

    pub fn rollback_last_batch(&mut self) -> Result<Option<String>, String> {
        let Some(batch) = self.last_batch.take() else {
            return Ok(None);
        };
        for entry in &batch.entries {
            let block = &mut self.blocks[entry.block_index];
            let path = self.root.join(&block.file_path);
            match &entry.previous_content {
                Some(content) => {
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
                    }
                    fs::write(&path, content).map_err(|err| err.to_string())?;
                }
                None => {
                    let _ = fs::remove_file(&path);
                }
            }
            block.status = entry.previous_status;
            block.selected_for_batch = false;
        }
        self.refresh_groups();
        Ok(Some(format!("Rolled back: {}", batch.batch_id)))
    }

    fn selected_block_indices(&self) -> Vec<usize> {
        self.blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| {
                block.selected_for_batch && matches!(block.status, EditBlockStatus::Pending)
            })
            .map(|(index, _)| index)
            .collect()
    }

    fn focus_current_group(&mut self) {
        if let Some(group) = self.groups.get(self.current_group)
            && let Some(first) = group.block_indices.first()
        {
            self.focused_block = *first;
        }
    }
}

fn build_groups(blocks: &[EditBlock], grouping: ReviewGroupingStrategy) -> Vec<ReviewGroup> {
    let mut grouped = BTreeMap::<String, Vec<usize>>::new();
    for (index, block) in blocks.iter().enumerate() {
        let title = match grouping {
            ReviewGroupingStrategy::ByCrate => block.crate_name.clone(),
            ReviewGroupingStrategy::ByTargetDirectory => block.target_directory.clone(),
            ReviewGroupingStrategy::ByRiskLabel => block.risk_label.clone(),
            ReviewGroupingStrategy::ByPatchFamily => block.patch_family.clone(),
        };
        grouped.entry(title).or_default().push(index);
    }
    let mut groups = grouped
        .into_iter()
        .map(|(title, block_indices)| ReviewGroup {
            selected_count: block_indices
                .iter()
                .filter(|index| blocks[**index].selected_for_batch)
                .count(),
            aggregate_risk: aggregate_risk(blocks, &block_indices),
            title,
            block_indices,
        })
        .collect::<Vec<_>>();
    groups.sort_by(|left, right| {
        risk_order(left.aggregate_risk.as_str())
            .cmp(&risk_order(right.aggregate_risk.as_str()))
            .then(left.title.cmp(&right.title))
    });
    groups
}

fn aggregate_risk(blocks: &[EditBlock], indices: &[usize]) -> String {
    indices
        .iter()
        .map(|index| blocks[*index].risk_label.as_str())
        .min_by_key(|label| risk_order(label))
        .unwrap_or("low")
        .to_string()
}

fn risk_order(label: &str) -> u8 {
    match label {
        "high" => 0,
        "medium" => 1,
        _ => 2,
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;
    use crate::coding::{
        ChangeSummary, ChangeType, CodeChange, CodeChangeSet, CodingExecutionResult, DiffHunk,
    };

    #[test]
    fn batch_apply_selected_updates_only_selected_blocks() {
        let root = temp_root("batch_apply_selected");
        fs::create_dir_all(root.join("crates/core/src")).expect("mkdir core");
        fs::write(root.join("crates/core/src/lib.rs"), "old core\n").expect("write core");
        fs::create_dir_all(root.join("apps/cli/src")).expect("mkdir apps");
        fs::write(root.join("apps/cli/src/repl.rs"), "old repl\n").expect("write repl");
        let report = sample_report(
            &root,
            vec![
                (
                    "crates/core/src/lib.rs",
                    "pub fn new_core() {}\n",
                    ChangeType::ModifyFile,
                ),
                (
                    "apps/cli/src/repl.rs",
                    "fn new_repl() {}\n",
                    ChangeType::ModifyFile,
                ),
            ],
        );
        let mut state = ReviewBatchState::from_coding_reports(&[(report, "family".to_string())])
            .expect("state")
            .expect("some");
        state.toggle_batch_selected();
        let summary = state
            .apply_selected_batch()
            .expect("apply batch")
            .expect("summary");
        assert!(summary.contains("Applied batch:"));
        assert_eq!(
            fs::read_to_string(root.join("crates/core/src/lib.rs")).expect("read core"),
            "pub fn new_core() {}\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("apps/cli/src/repl.rs")).expect("read repl"),
            "old repl\n"
        );
    }

    #[test]
    fn batch_discard_removes_only_selected_pending_blocks() {
        let root = temp_root("batch_discard_selected");
        let report = sample_report(
            &root,
            vec![
                (
                    "crates/core/src/lib.rs",
                    "pub fn new_core() {}\n",
                    ChangeType::ModifyFile,
                ),
                (
                    "apps/cli/src/repl.rs",
                    "fn new_repl() {}\n",
                    ChangeType::ModifyFile,
                ),
            ],
        );
        let mut state = ReviewBatchState::from_coding_reports(&[(report, "family".to_string())])
            .expect("state")
            .expect("some");
        state.toggle_batch_selected();
        let summary = state.discard_selected_batch().expect("discard summary");
        assert_eq!(summary, "Discarded selected: 1 blocks");
        assert!(matches!(state.blocks[0].status, EditBlockStatus::Discarded));
        assert!(matches!(state.blocks[1].status, EditBlockStatus::Pending));
    }

    #[test]
    fn rollback_last_batch_restores_previous_pending_state() {
        let root = temp_root("rollback_last_batch");
        fs::create_dir_all(root.join("crates/core/src")).expect("mkdir");
        fs::write(root.join("crates/core/src/lib.rs"), "old core\n").expect("write");
        let report = sample_report(
            &root,
            vec![(
                "crates/core/src/lib.rs",
                "pub fn new_core() {}\n",
                ChangeType::ModifyFile,
            )],
        );
        let mut state = ReviewBatchState::from_coding_reports(&[(report, "family".to_string())])
            .expect("state")
            .expect("some");
        state.toggle_batch_selected();
        state
            .apply_selected_batch()
            .expect("apply")
            .expect("summary");
        let summary = state
            .rollback_last_batch()
            .expect("rollback")
            .expect("summary");
        assert!(summary.starts_with("Rolled back: batch-"));
        assert!(matches!(state.blocks[0].status, EditBlockStatus::Pending));
        assert_eq!(
            fs::read_to_string(root.join("crates/core/src/lib.rs")).expect("read"),
            "old core\n"
        );
    }

    #[test]
    fn group_navigation_preserves_selection_and_focus() {
        let root = temp_root("group_navigation");
        let report = sample_report(
            &root,
            vec![
                (
                    "crates/core/src/lib.rs",
                    "pub fn new_core() {}\n",
                    ChangeType::ModifyFile,
                ),
                (
                    "apps/cli/src/repl.rs",
                    "fn new_repl() {}\n",
                    ChangeType::ModifyFile,
                ),
            ],
        );
        let mut state = ReviewBatchState::from_coding_reports(&[(report, "family".to_string())])
            .expect("state")
            .expect("some");
        state.toggle_batch_selected();
        let focused_before = state.focused_block;
        state.set_grouping(ReviewGroupingStrategy::ByCrate);
        state.next_group();
        assert_eq!(state.selected_pending_count(), 1);
        state.previous_group();
        assert_eq!(state.selected_pending_count(), 1);
        assert_ne!(state.focused_block, usize::MAX);
        assert!(state.blocks[focused_before].selected_for_batch);
    }

    fn sample_report(
        root: &std::path::Path,
        changes: Vec<(&str, &str, ChangeType)>,
    ) -> CodingReviewReport {
        for (path, content, _) in &changes {
            let full = root.join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).expect("mkdir");
            }
            if !full.exists() {
                fs::write(&full, "old\n").expect("seed");
            } else {
                let _ = content;
            }
        }
        let change_count = changes.len();
        CodingReviewReport {
            root: root.display().to_string(),
            execution: sample_execution_result(),
            changes: CodeChangeSet {
                patches: vec![],
                changes: changes
                    .into_iter()
                    .map(|(file, replacement, change_type)| CodeChange {
                        file_path: file.to_string(),
                        change_type,
                        hunks: vec![DiffHunk {
                            start_line: 1,
                            end_line: 1,
                            replacement: replacement.to_string(),
                        }],
                    })
                    .collect(),
                summary: ChangeSummary {
                    total_changes: change_count,
                    create_files: 0,
                    modify_files: change_count,
                    move_files: 0,
                },
                canonical_target: None,
            },
        }
    }

    fn sample_execution_result() -> CodingExecutionResult {
        CodingExecutionResult {
            status: "checked".to_string(),
            applied: false,
            checked: true,
            build_fixed: false,
            build_ok: true,
            rolled_back: false,
            backed_up: false,
            reason: None,
            sandbox_root: None,
            files_changed: 0,
            diff: crate::coding::DiffReport::default(),
            committed: false,
            commit_id: None,
            branch: None,
            transactional_apply: None,
            git_commit: None,
            git_push: None,
            pull_request: None,
            canonical_target_path: None,
            legacy_pipeline_hits: 0,
            fallback_resolution_hits: 0,
            stale_artifact_detected: false,
        }
    }

    fn temp_root(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("design_cli_{label}_{unique}"));
        fs::create_dir_all(&root).expect("create temp root");
        root
    }
}
