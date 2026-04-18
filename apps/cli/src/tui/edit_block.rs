use std::path::{Path, PathBuf};

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use serde::Deserialize;

use crate::coding::{
    CodeChange, CodeChangeSet, CodingExecutionResult, DiffHunk, build_unified_diff_preview,
    render_code_diff_lines,
};
use crate::tui::review_batch::ReviewBatchState;
const COLLAPSED_DIFF_LINES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CodingReviewReport {
    pub root: String,
    pub execution: CodingExecutionResult,
    pub changes: CodeChangeSet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditBlockStatus {
    Pending,
    Applied,
    Discarded,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditBlock {
    pub patch_id: String,
    pub file_path: String,
    pub confidence_label: String,
    pub confidence_score: f32,
    pub risk_label: String,
    pub hunk_count: usize,
    pub operation: String,
    pub diff_lines: Vec<String>,
    pub replacement: String,
    pub added_lines: usize,
    pub removed_lines: usize,
    pub expanded: bool,
    pub status: EditBlockStatus,
    pub selected_for_batch: bool,
    pub patch_family: String,
    pub crate_name: String,
    pub target_directory: String,
    pub destructive: bool,
    pub cross_crate: bool,
}

pub fn render_edit_blocks(frame: &mut Frame, review: &ReviewBatchState, area: Rect) {
    if review.blocks.is_empty() || area.height == 0 {
        return;
    }

    let mut y = area.y;
    for (group_index, group) in review.groups.iter().enumerate() {
        if y >= area.y + area.height {
            break;
        }
        let group_height = 3u16;
        if y + group_height > area.y + area.height {
            break;
        }
        let group_rect = Rect {
            x: area.x,
            y,
            width: area.width,
            height: group_height,
        };
        render_group_header(
            frame,
            review,
            group,
            group_index == review.current_group,
            group_rect,
        );
        y = y.saturating_add(group_height);

        for block_index in &group.block_indices {
            if y >= area.y + area.height {
                break;
            }
            let block = &review.blocks[*block_index];
            let visible_lines = block_visible_lines(block);
            let height = (visible_lines.len() as u16 + 4).min(area.y + area.height - y);
            let rect = Rect {
                x: area.x,
                y,
                width: area.width,
                height,
            };
            render_edit_block(frame, block, rect, *block_index == review.focused_block);
            y = y.saturating_add(height);
        }
    }
}

pub fn block_visible_lines_for_test(block: &EditBlock) -> Vec<String> {
    block_visible_lines(block)
}

pub fn group_header_summary_for_test(review: &ReviewBatchState, group_index: usize) -> String {
    review
        .groups
        .get(group_index)
        .map(|group| group_header_summary(review, group))
        .unwrap_or_default()
}

pub fn build_edit_blocks(
    report: &CodingReviewReport,
    patch_family: &str,
) -> Result<Vec<EditBlock>, String> {
    let root = PathBuf::from(&report.root);
    let mut blocks = Vec::new();
    for (index, change) in report.changes.changes.iter().enumerate() {
        let rank = crate::tui::confidence_rank::rank_edit_block(
            &change.file_path,
            operation_label(change).as_str(),
            change
                .hunks
                .last()
                .map(|hunk| hunk.replacement.as_str())
                .unwrap_or_default(),
        );
        let diff = build_unified_diff_preview(
            &root,
            &CodeChangeSet {
                patches: vec![],
                changes: vec![change.clone()],
                summary: crate::coding::ChangeSummary {
                    total_changes: 1,
                    create_files: usize::from(matches!(
                        change.change_type,
                        crate::coding::ChangeType::CreateFile
                    )),
                    modify_files: usize::from(matches!(
                        change.change_type,
                        crate::coding::ChangeType::ModifyFile
                    )),
                    move_files: usize::from(matches!(
                        change.change_type,
                        crate::coding::ChangeType::MoveFile
                    )),
                },
                canonical_target: Some(PathBuf::from(&change.file_path)),
            },
        )?;
        let rendered_diff = diff
            .files
            .first()
            .map(render_code_diff_lines)
            .unwrap_or_default();
        let diff_counts = diff.files.first();
        blocks.push(EditBlock {
            patch_id: deterministic_patch_id(index, change),
            file_path: change.file_path.clone(),
            confidence_label: rank.label.to_string(),
            confidence_score: rank.score,
            risk_label: rank.risk_label.to_string(),
            hunk_count: count_hunks(change),
            operation: operation_label(change),
            diff_lines: rendered_diff,
            replacement: change
                .hunks
                .last()
                .map(|hunk| hunk.replacement.clone())
                .unwrap_or_default(),
            added_lines: diff_counts.map(|entry| entry.added_lines).unwrap_or(0),
            removed_lines: diff_counts.map(|entry| entry.removed_lines).unwrap_or(0),
            expanded: false,
            status: EditBlockStatus::Pending,
            selected_for_batch: false,
            patch_family: patch_family.to_string(),
            crate_name: crate_name_for_path(&change.file_path),
            target_directory: target_directory_for_path(&change.file_path),
            destructive: rank.destructive,
            cross_crate: rank.cross_crate,
        });
    }
    Ok(blocks)
}

pub fn change_set_for_block(block: &EditBlock) -> CodeChangeSet {
    let change_type = match block.operation.as_str() {
        "create" => crate::coding::ChangeType::CreateFile,
        "delete" => crate::coding::ChangeType::MoveFile,
        _ => crate::coding::ChangeType::ModifyFile,
    };
    CodeChangeSet {
        patches: vec![],
        changes: vec![CodeChange {
            file_path: block.file_path.clone(),
            change_type,
            hunks: vec![DiffHunk {
                start_line: 1,
                end_line: 1,
                replacement: block.replacement.clone(),
            }],
        }],
        summary: crate::coding::ChangeSummary {
            total_changes: 1,
            create_files: usize::from(block.operation == "create"),
            modify_files: usize::from(block.operation == "modify"),
            move_files: usize::from(block.operation == "delete"),
        },
        canonical_target: Some(PathBuf::from(&block.file_path)),
    }
}

fn render_group_header(
    frame: &mut Frame,
    review: &ReviewBatchState,
    group: &crate::tui::review_batch::ReviewGroup,
    focused: bool,
    area: Rect,
) {
    let border_style = if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let line = Line::from(vec![Span::raw(group_header_summary(review, group))]);
    frame.render_widget(
        Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style),
        ),
        area,
    );
}

fn group_header_summary(
    review: &ReviewBatchState,
    group: &crate::tui::review_batch::ReviewGroup,
) -> String {
    let representative = group
        .block_indices
        .first()
        .and_then(|index| review.blocks.get(*index));
    let mut summary = format!(
        "[group:{}] {}  blocks={}  selected={}",
        review.grouping.as_str(),
        group.title,
        group.block_indices.len(),
        group.selected_count,
    );
    if let Some(block) = representative {
        summary.push_str(&format!(
            "  file={}  hunks={}  confidence={} ({:.2})  risk={}  type={}",
            block.file_path,
            block.hunk_count,
            block.confidence_label,
            block.confidence_score,
            block.risk_label,
            block.operation,
        ));
    } else {
        summary.push_str(&format!("  risk={}", group.aggregate_risk));
    }
    summary
}

fn render_edit_block(frame: &mut Frame, block: &EditBlock, area: Rect, focused: bool) {
    let status_badge = match block.status {
        EditBlockStatus::Pending => "pending",
        EditBlockStatus::Applied => "applied",
        EditBlockStatus::Discarded => "discarded",
    };
    let title = format!(
        " {} | conf={} ({:.2}) | {} hunks | {} | +{} -{} | {} ",
        block.file_path,
        block.confidence_label,
        block.confidence_score,
        block.hunk_count,
        block.operation,
        block.added_lines,
        block.removed_lines,
        block.patch_id
    );
    let border_style = if focused {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let dimmed = matches!(
        block.status,
        EditBlockStatus::Applied | EditBlockStatus::Discarded
    );
    let block_style = if dimmed {
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM)
    } else {
        Style::default()
    };
    let mut lines = vec![Line::from(vec![
        Span::styled(
            format!("[{}]", status_badge),
            match block.status {
                EditBlockStatus::Pending => Style::default().fg(Color::Yellow),
                EditBlockStatus::Applied => Style::default().fg(Color::Green),
                EditBlockStatus::Discarded => Style::default().fg(Color::Red),
            },
        ),
        Span::raw(" "),
        Span::styled(
            if block.selected_for_batch {
                "[selected]"
            } else {
                "[ ]"
            },
            Style::default().fg(Color::Cyan),
        ),
        Span::raw(" "),
        Span::styled(
            format!("crate={} dir={}", block.crate_name, block.target_directory),
            Style::default().fg(Color::DarkGray),
        ),
    ])];
    lines.extend(
        block_visible_lines(block)
            .into_iter()
            .map(|line| Line::from(styled_diff_line(&line))),
    );
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(title)
                    .border_style(border_style),
            )
            .style(block_style)
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn block_visible_lines(block: &EditBlock) -> Vec<String> {
    if block.expanded || block.diff_lines.len() <= COLLAPSED_DIFF_LINES {
        return block.diff_lines.clone();
    }
    let mut lines = block
        .diff_lines
        .iter()
        .take(COLLAPSED_DIFF_LINES)
        .cloned()
        .collect::<Vec<_>>();
    lines.push("...".to_string());
    lines
}

fn styled_diff_line(line: &str) -> Vec<Span<'static>> {
    let style = if line.starts_with('+') && !line.starts_with("+++") {
        Style::default().fg(Color::Green)
    } else if line.starts_with('-') && !line.starts_with("---") {
        Style::default().fg(Color::Red)
    } else if line.starts_with("@@") || line.starts_with("---") || line.starts_with("+++") {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    vec![Span::styled(line.to_string(), style)]
}

fn operation_label(change: &CodeChange) -> String {
    match change.change_type {
        crate::coding::ChangeType::CreateFile => "create".to_string(),
        crate::coding::ChangeType::ModifyFile => "modify".to_string(),
        crate::coding::ChangeType::MoveFile => "delete".to_string(),
    }
}

fn count_hunks(change: &CodeChange) -> usize {
    change.hunks.len().max(1)
}

fn deterministic_patch_id(index: usize, change: &CodeChange) -> String {
    format!(
        "patch-{}-{}-{}",
        index,
        change.file_path.replace('/', "_"),
        change.hunks.len()
    )
}

fn crate_name_for_path(path: &str) -> String {
    let parts = path.split('/').collect::<Vec<_>>();
    if parts.first() == Some(&"crates") && parts.len() > 1 {
        return parts[1].to_string();
    }
    if parts.first() == Some(&"apps") && parts.len() > 1 {
        return parts[1].to_string();
    }
    parts.first().copied().unwrap_or(".").to_string()
}

fn target_directory_for_path(path: &str) -> String {
    Path::new(path)
        .parent()
        .map(|parent| parent.display().to_string())
        .unwrap_or_else(|| ".".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_block_renders_unified_diff_with_context_three() {
        let line = styled_diff_line("@@ -1,7 +1,7 @@");
        assert_eq!(line[0].style.fg, Some(Color::Cyan));
        let context = styled_diff_line(" unchanged");
        assert_eq!(context[0].style.fg, Some(Color::DarkGray));
    }
}
