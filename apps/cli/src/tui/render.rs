use std::collections::BTreeMap;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};

use super::model::{HypothesisViewModel, ScorePartsViewModel};
use super::state::{ActivePanel, TuiState};

// ── Layout ────────────────────────────────────────────────────────────────────
//
// ┌──────────────────────────────────┐  30%  Trace Timeline + stats footer
// ├──────────────────┬───────────────┤
// │  Hypothesis DAG  │  Score Panel  │  45%
// ├──────────────────┴───────────────┤
// │  Memory / Recall Panel           │  25%
// └──────────────────────────────────┘

pub fn render(frame: &mut Frame, state: &mut TuiState) {
    let area = frame.area();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(30), // Trace
            Constraint::Percentage(45), // Hypothesis + Score
            Constraint::Percentage(25), // Memory
        ])
        .split(area);

    let mid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(rows[1]);

    render_trace(frame, state, rows[0]);
    render_hypothesis_graph(frame, state, mid[0]);
    render_score_panel(frame, state, mid[1]);
    render_memory_panel(frame, state, rows[2]);
    render_help_bar(frame, area);
}

// ── Trace Timeline ─────────────────────────────────────────────────────────────

fn render_trace(frame: &mut Frame, state: &mut TuiState, area: Rect) {
    let is_active = state.active_panel == ActivePanel::Trace;

    // Reserve 1 row at bottom of the block for stats footer.
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Trace Timeline ")
        .border_style(active_border(is_active));

    let steps = &state.payload.trace.steps;
    let selected_step = state.selected_step;

    let items: Vec<ListItem> = steps
        .iter()
        .enumerate()
        .map(|(i, step)| {
            let is_sel = i == selected_step;
            let prefix = if is_sel { "▶ " } else { "  " };

            // Pruned bar: visual fraction of candidates removed
            let kept = step.candidates.saturating_sub(step.pruned);
            let prune_ratio = if step.candidates > 0 {
                step.pruned as f32 / step.candidates as f32
            } else {
                0.0
            };
            let prune_icon = if prune_ratio > 0.5 {
                "▼▼"
            } else if prune_ratio > 0.0 {
                "▼ "
            } else {
                "  "
            };

            let text = format!(
                "{}{} depth={:<2} beam={:<2} cand={:<3} kept={:<3} pruned={:<3} {} recall={}",
                prefix,
                prune_icon,
                step.depth,
                step.beam_width,
                step.candidates,
                kept,
                step.pruned,
                "│",
                step.recall_hits,
            );

            let style = if is_sel {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect();

    // Build inner area minus the stats footer row.
    let inner = block.inner(area);
    let stats_y = inner.y + inner.height.saturating_sub(1);
    let list_area = Rect {
        height: inner.height.saturating_sub(1),
        ..inner
    };
    let stats_area = Rect {
        y: stats_y,
        height: 1,
        ..inner
    };

    frame.render_widget(block, area);

    let mut list_state = ListState::default();
    list_state.select(Some(selected_step));
    frame.render_stateful_widget(List::new(items), list_area, &mut list_state);

    // Stats footer
    let s = &state.payload.trace.stats;
    let stats_text = format!(
        " Nodes:{} Depth:{} Recall:{:.2} Branch:{:.1}  (score desc)",
        s.total_nodes, s.max_depth, s.recall_hit_rate, s.avg_branching
    );
    frame.render_widget(
        Paragraph::new(stats_text).style(Style::default().fg(Color::DarkGray)),
        stats_area,
    );
}

// ── Hypothesis DAG ─────────────────────────────────────────────────────────────

/// A pre-processed line in the tree view.
struct TreeLine {
    prefix: String,
    id: usize,
    score: f32,
    depth: usize,
}

fn build_tree_lines(hypotheses: &[HypothesisViewModel]) -> Vec<TreeLine> {
    // parent_id → ordered children ids
    let mut children: BTreeMap<Option<usize>, Vec<usize>> = BTreeMap::new();
    for h in hypotheses {
        children.entry(h.parent).or_default().push(h.id);
    }

    let mut lines = Vec::new();
    if let Some(roots) = children.get(&None) {
        let root_ids = roots.clone();
        let n = root_ids.len();
        for (i, &root_id) in root_ids.iter().enumerate() {
            collect_tree(root_id, "", i + 1 == n, &children, hypotheses, &mut lines);
        }
    }
    lines
}

fn collect_tree(
    id: usize,
    prefix: &str,
    is_last: bool,
    children: &BTreeMap<Option<usize>, Vec<usize>>,
    hypotheses: &[HypothesisViewModel],
    lines: &mut Vec<TreeLine>,
) {
    let hyp = match hypotheses.iter().find(|h| h.id == id) {
        Some(h) => h,
        None => return,
    };

    let connector = if prefix.is_empty() {
        ""
    } else if is_last {
        "└─ "
    } else {
        "├─ "
    };
    let full_prefix = format!("{}{}", prefix, connector);

    lines.push(TreeLine {
        prefix: full_prefix,
        id,
        score: hyp.score,
        depth: hyp.depth,
    });

    let child_prefix = if prefix.is_empty() {
        String::new()
    } else if is_last {
        format!("{}   ", prefix)
    } else {
        format!("{}│  ", prefix)
    };

    if let Some(child_ids) = children.get(&Some(id)) {
        let child_ids = child_ids.clone();
        let n = child_ids.len();
        for (i, &cid) in child_ids.iter().enumerate() {
            collect_tree(cid, &child_prefix, i + 1 == n, children, hypotheses, lines);
        }
    }
}

fn render_hypothesis_graph(frame: &mut Frame, state: &mut TuiState, area: Rect) {
    let is_active = state.active_panel == ActivePanel::Hypothesis;

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Hypothesis Graph (score desc) ")
        .border_style(active_border(is_active));

    let selected_id = state.selected_hypothesis;
    let highlighted_depth = state.selected_depth();

    let tree_lines = build_tree_lines(&state.payload.hypotheses);

    // Collect DAG cross-links from all hypotheses.
    let cross_links: Vec<(usize, usize, &str)> = state
        .payload
        .hypotheses
        .iter()
        .flat_map(|h| {
            h.relations
                .iter()
                .map(move |r| (h.id, r.to_id, r.relation_type.as_str()))
        })
        .collect();

    // How many rows the cross-link section needs (min 1 blank separator).
    let cross_rows = if cross_links.is_empty() { 0 } else { cross_links.len() + 2 };

    let inner = block.inner(area);
    let tree_height = inner.height.saturating_sub(cross_rows as u16);
    let tree_area = Rect { height: tree_height, ..inner };
    let link_area = Rect {
        y: inner.y + tree_height,
        height: inner.height - tree_height,
        ..inner
    };

    frame.render_widget(block, area);

    // Tree list
    let items: Vec<ListItem> = tree_lines
        .iter()
        .map(|line| {
            let is_sel = selected_id == Some(line.id);
            let is_depth = highlighted_depth == Some(line.depth);

            let text = format!("{}H{} ({:.2})", line.prefix, line.id, line.score);

            let style = if is_sel {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else if is_depth {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::Gray)
            };

            ListItem::new(text).style(style)
        })
        .collect();

    let list_index = tree_lines.iter().position(|l| Some(l.id) == state.selected_hypothesis);

    let mut list_state = ListState::default();
    list_state.select(list_index);
    frame.render_stateful_widget(List::new(items), tree_area, &mut list_state);

    // Cross-link section
    if !cross_links.is_empty() {
        let mut link_text = String::from("─ Cross-Links ─\n");
        for (from, to, rel) in &cross_links {
            link_text.push_str(&format!(" H{} → H{} ({})\n", from, to, rel));
        }
        frame.render_widget(
            Paragraph::new(link_text).style(Style::default().fg(Color::Magenta)),
            link_area,
        );
    }
}

// ── Score Panel ────────────────────────────────────────────────────────────────

fn render_score_panel(frame: &mut Frame, state: &TuiState, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Score Panel ")
        .border_style(Style::default().fg(Color::DarkGray));

    if let Some(hyp) = state.selected_hypothesis_data() {
        let bar_width = (area.width as usize).saturating_sub(4).min(20).max(8);
        let content = build_score_content(hyp.id, hyp.score, &hyp.score_parts, bar_width);
        frame.render_widget(
            Paragraph::new(content).block(block).wrap(Wrap { trim: false }),
            area,
        );
    } else {
        frame.render_widget(
            Paragraph::new("  Select a hypothesis")
                .block(block)
                .style(Style::default().fg(Color::DarkGray)),
            area,
        );
    }
}

fn build_score_content(id: usize, total: f32, parts: &ScorePartsViewModel, bar_width: usize) -> String {
    let mut out = format!("  H{id}\n  Total: {total:.2}\n\n");
    out.push_str(&score_bar("relevance ", parts.relevance, bar_width));
    out.push_str(&score_bar("goal      ", parts.goal, bar_width));
    out.push_str(&score_bar("constraint", parts.constraint, bar_width));
    out.push_str(&score_bar("memory    ", parts.memory, bar_width));
    out
}

fn score_bar(label: &str, value: f32, width: usize) -> String {
    let clamped = value.clamp(0.0, 1.0);
    let filled = ((clamped * width as f32).round() as usize).min(width);
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(width - filled));
    format!("  {label} {bar} {value:.2}\n")
}

// ── Memory / Recall Panel ──────────────────────────────────────────────────────

fn render_memory_panel(frame: &mut Frame, state: &TuiState, area: Rect) {
    let is_active = state.active_panel == ActivePanel::Memory;

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Memory / Recall Candidates ")
        .border_style(active_border(is_active));

    let candidates = state.visible_memory();

    if candidates.is_empty() {
        frame.render_widget(
            Paragraph::new("  No recall candidates")
                .block(block)
                .style(Style::default().fg(Color::DarkGray)),
            area,
        );
        return;
    }

    let bar_width = (area.width as usize).saturating_sub(38).min(16).max(6);

    let items: Vec<ListItem> = candidates
        .iter()
        .skip(state.memory_scroll)
        .enumerate()
        .map(|(_i, m)| {
            let bar = memory_bar(m.score, bar_width);
            let source_color = match m.source.as_str() {
                "exact" => Color::Green,
                "cache" => Color::Cyan,
                _ => Color::Gray,
            };
            let tags_preview = if m.tags.is_empty() {
                String::new()
            } else {
                format!(" [{}]", m.tags.join(", "))
            };
            let text = format!(
                " M{:<2} {} {:.2} {:6}{}",
                m.rank + 1,
                bar,
                m.score,
                m.source,
                tags_preview,
            );
            ListItem::new(text).style(Style::default().fg(source_color))
        })
        .collect();

    let mut list_state = ListState::default();
    frame.render_stateful_widget(List::new(items).block(block), area, &mut list_state);
}

fn memory_bar(value: f32, width: usize) -> String {
    let clamped = value.clamp(0.0, 1.0);
    let filled = ((clamped * width as f32).round() as usize).min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

// ── Help bar ──────────────────────────────────────────────────────────────────

fn render_help_bar(frame: &mut Frame, area: Rect) {
    let help_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };
    frame.render_widget(
        Paragraph::new(
            " ↑↓ navigate   Tab cycle panel [Trace→Graph→Memory]   q quit ",
        )
        .style(Style::default().fg(Color::DarkGray)),
        help_area,
    );
}

// ── Helpers ────────────────────────────────────────────────────────────────────

fn active_border(active: bool) -> Style {
    if active {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}
