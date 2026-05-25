//! memory_admin — MemorySpace 永続化記憶 管理者用 TUI
//!
//! 永続化記憶の最適化システムを管理・監視するための対話型ツール。
//! 実行コマンド: `cargo run --bin memory_admin -- [--store <path>] [--demo]`
//!
//! ## キーバインド
//!
//! | キー         | 動作                                        |
//! |-------------|---------------------------------------------|
//! | Tab / Shift+Tab | タブ切り替え (Overview → Memories → Audit → Search) |
//! | ↑ / k       | リスト上移動                                |
//! | ↓ / j       | リスト下移動                                |
//! | /           | 検索モード開始                              |
//! | Enter       | 詳細表示 / 検索実行                         |
//! | s           | ストアをファイルに保存 (--store 指定時)      |
//! | p           | 刈り込みダイアログ                          |
//! | r           | 刈り込み実行 (confirm 時)                   |
//! | Esc         | 検索キャンセル / ダイアログ閉じる           |
//! | q           | 終了 (--store 指定時は自動保存して終了)     |

#![allow(dead_code)]

use std::ffi::OsString;
use std::io::{self};
use std::path::PathBuf;
use std::time::Duration;

use clap::error::ErrorKind;
use clap::{Parser, Subcommand};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use memory_engine::MemoryRecord;
use memory_persistence::{
    DecisionPolicy, GeneralizedMemory, IngestResult, OptimizationStats, PersistentMemoryStore,
};
use memory_space_core::{
    SemanticIdentityCandidate, SemanticIdentityGraph, semantic_rewrite_transaction,
    semantic_rollback_snapshot,
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

// ── CLI args ──────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "design_cli",
    about = "DBM MemorySpace 永続化記憶 管理者用 TUI",
    version = env!("CARGO_PKG_VERSION")
)]
struct Args {
    /// 永続化ファイルのパス (省略時はオンメモリのみ)
    #[arg(long = "store", value_name = "PATH", global = true)]
    store: Option<PathBuf>,

    /// デモデータでストアを初期化する
    #[arg(long = "demo", default_value_t = false, global = true)]
    demo: bool,

    #[command(subcommand)]
    command: Option<MemoryCommand>,
}

#[derive(Subcommand, Debug)]
enum MemoryCommand {
    /// Preview, validate, or apply a semantic rewrite transaction.
    Rewrite {
        #[arg(long = "preview", conflicts_with_all = ["validate", "apply"])]
        preview: bool,

        #[arg(long = "validate", conflicts_with_all = ["preview", "apply"])]
        validate: bool,

        #[arg(long = "apply", conflicts_with_all = ["preview", "validate"])]
        apply: bool,

        /// Required operator confirmation for semantic apply.
        #[arg(long = "yes", default_value_t = false)]
        yes: bool,
    },

    /// Restore the current semantic rollback snapshot through runtime rollback.
    Rollback,

    /// Inspect the deterministic semantic topology projection.
    Topology,

    /// Inspect deterministic semantic drift projection.
    Drift,

    /// Inspect deterministic semantic attractors.
    Attractors,
}

// ── State ──────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Overview,
    Memories,
    Audit,
    Search,
}

impl Tab {
    fn next(self) -> Self {
        match self {
            Tab::Overview => Tab::Memories,
            Tab::Memories => Tab::Audit,
            Tab::Audit => Tab::Search,
            Tab::Search => Tab::Overview,
        }
    }
    fn prev(self) -> Self {
        match self {
            Tab::Overview => Tab::Search,
            Tab::Memories => Tab::Overview,
            Tab::Audit => Tab::Memories,
            Tab::Search => Tab::Audit,
        }
    }
    fn label(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Memories => "Memories",
            Tab::Audit => "Audit Log",
            Tab::Search => "Search",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModalDialog {
    None,
    PruneConfirm,
    MemoryDetail,
}

struct AppState {
    store: PersistentMemoryStore,
    active_tab: Tab,

    /// 永続化ファイルのパス (None = オンメモリのみ)
    store_path: Option<PathBuf>,
    /// 未保存の変更があるか
    dirty: bool,

    /// Memories タブのリスト選択インデックス
    memory_list_idx: usize,
    memory_scroll: usize,

    /// Audit タブのスクロール
    audit_scroll: usize,

    /// Search タブ
    search_query: String,
    search_input_mode: bool,
    search_results: Vec<(String, f32)>, // (memory_id, score)
    search_list_idx: usize,

    /// 詳細表示モーダル
    modal: ModalDialog,
    detail_memory_id: Option<String>,

    /// 刈り込み設定
    prune_min_recall: usize,

    /// ステータスバーのメッセージ
    status_msg: String,
}

impl AppState {
    fn new(store: PersistentMemoryStore, store_path: Option<PathBuf>) -> Self {
        let status_msg = match &store_path {
            Some(p) => format!("Loaded from {}. s=save  q=quit(auto-save)", p.display()),
            None => "No store file. Changes are in-memory only. Use --store <path> to persist."
                .to_string(),
        };
        Self {
            store,
            active_tab: Tab::Overview,
            store_path,
            dirty: false,
            memory_list_idx: 0,
            memory_scroll: 0,
            audit_scroll: 0,
            search_query: String::new(),
            search_input_mode: false,
            search_results: Vec::new(),
            search_list_idx: 0,
            modal: ModalDialog::None,
            detail_memory_id: None,
            prune_min_recall: 0,
            status_msg,
        }
    }

    /// ストアをファイルに保存する。store_path が None の場合は何もしない。
    fn save_to_file(&mut self) {
        let Some(path) = &self.store_path else {
            self.status_msg = "No store file configured (use --store <path>).".to_string();
            return;
        };
        match self.store.save(path) {
            Ok(()) => {
                self.dirty = false;
                self.status_msg = format!("Saved to {}.", path.display());
            }
            Err(e) => {
                self.status_msg = format!("Save error: {e}");
            }
        }
    }

    fn selected_memory(&self) -> Option<&GeneralizedMemory> {
        self.store.list().get(self.memory_list_idx)
    }

    fn run_search(&mut self) {
        let results = self
            .store
            .search(&self.search_query)
            .into_iter()
            .map(|(m, s)| (m.id.clone(), s))
            .collect();
        self.search_results = results;
        self.search_list_idx = 0;
        self.status_msg = format!(
            "Search '{}': {} results",
            self.search_query,
            self.search_results.len()
        );
    }

    fn execute_prune(&mut self) {
        let removed = self.store.prune_stale();
        self.dirty = removed > 0;
        self.status_msg = format!("Pruned {} stale memories.", removed);
        self.modal = ModalDialog::None;
        self.memory_list_idx = 0;
    }
}

// ── Key handling ──────────────────────────────────────────────────────────────

fn handle_key(state: &mut AppState, key: event::KeyEvent) -> bool {
    // 検索入力モード
    if state.search_input_mode {
        match key.code {
            KeyCode::Esc => {
                state.search_input_mode = false;
                state.status_msg = "Search cancelled.".to_string();
            }
            KeyCode::Enter => {
                state.search_input_mode = false;
                state.run_search();
            }
            KeyCode::Backspace => {
                state.search_query.pop();
            }
            KeyCode::Char(c) => {
                state.search_query.push(c);
            }
            _ => {}
        }
        return false;
    }

    // モーダルダイアログ
    match state.modal {
        ModalDialog::PruneConfirm => match key.code {
            KeyCode::Char('r') | KeyCode::Enter => {
                state.execute_prune();
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                state.modal = ModalDialog::None;
                state.status_msg = "Prune cancelled.".to_string();
            }
            _ => {}
        },
        ModalDialog::MemoryDetail => {
            if matches!(key.code, KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter) {
                state.modal = ModalDialog::None;
            }
        }
        ModalDialog::None => match key.code {
            KeyCode::Char('q') | KeyCode::Esc => return true,
            KeyCode::Tab => state.active_tab = state.active_tab.next(),
            KeyCode::BackTab => state.active_tab = state.active_tab.prev(),
            KeyCode::Up | KeyCode::Char('k') => handle_up(state),
            KeyCode::Down | KeyCode::Char('j') => handle_down(state),
            KeyCode::Enter => handle_enter(state),
            KeyCode::Char('/') => {
                state.active_tab = Tab::Search;
                state.search_input_mode = true;
                state.status_msg = "Type query then Enter...".to_string();
            }
            KeyCode::Char('s') => {
                state.save_to_file();
            }
            KeyCode::Char('p') => {
                state.modal = ModalDialog::PruneConfirm;
                state.status_msg =
                    "Prune stale memories (v1, recall=0)? [r] confirm [Esc] cancel".to_string();
            }
            _ => {}
        },
    }
    false
}

fn handle_up(state: &mut AppState) {
    match state.active_tab {
        Tab::Memories => {
            if state.memory_list_idx > 0 {
                state.memory_list_idx -= 1;
                state.memory_scroll = state.memory_scroll.min(state.memory_list_idx);
            }
        }
        Tab::Audit => {
            state.audit_scroll = state.audit_scroll.saturating_sub(1);
        }
        Tab::Search => {
            if state.search_list_idx > 0 {
                state.search_list_idx -= 1;
            }
        }
        _ => {}
    }
}

fn handle_down(state: &mut AppState) {
    match state.active_tab {
        Tab::Memories => {
            let max = state.store.memory_count().saturating_sub(1);
            if state.memory_list_idx < max {
                state.memory_list_idx += 1;
            }
        }
        Tab::Audit => {
            let max = state.store.audit_log().len().saturating_sub(1);
            if state.audit_scroll < max {
                state.audit_scroll += 1;
            }
        }
        Tab::Search => {
            let max = state.search_results.len().saturating_sub(1);
            if state.search_list_idx < max {
                state.search_list_idx += 1;
            }
        }
        _ => {}
    }
}

fn handle_enter(state: &mut AppState) {
    match state.active_tab {
        Tab::Memories => {
            if let Some(m) = state.selected_memory() {
                state.detail_memory_id = Some(m.id.clone());
                state.modal = ModalDialog::MemoryDetail;
            }
        }
        Tab::Search => {
            if let Some((id, _)) = state.search_results.get(state.search_list_idx) {
                state.detail_memory_id = Some(id.clone());
                state.modal = ModalDialog::MemoryDetail;
            }
        }
        _ => {}
    }
}

// ── Rendering ─────────────────────────────────────────────────────────────────

fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // タブバー
            Constraint::Min(0),    // コンテンツ
            Constraint::Length(1), // ステータスバー
        ])
        .split(area);

    render_tab_bar(frame, state, rows[0]);
    render_content(frame, state, rows[1]);
    render_status_bar(frame, state, rows[2]);

    if state.modal != ModalDialog::None {
        render_modal(frame, state, area);
    }
}

fn render_tab_bar(frame: &mut Frame, state: &AppState, area: Rect) {
    let tabs = [Tab::Overview, Tab::Memories, Tab::Audit, Tab::Search];
    let labels: Vec<String> = tabs
        .iter()
        .map(|&t| {
            if t == state.active_tab {
                format!(" [{}] ", t.label())
            } else {
                format!("  {}  ", t.label())
            }
        })
        .collect();
    let bar_text = labels.join("│");
    frame.render_widget(
        Paragraph::new(bar_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" memory_admin "),
            )
            .style(Style::default().fg(Color::Cyan)),
        area,
    );
}

fn render_content(frame: &mut Frame, state: &AppState, area: Rect) {
    match state.active_tab {
        Tab::Overview => render_overview(frame, state, area),
        Tab::Memories => render_memories(frame, state, area),
        Tab::Audit => render_audit(frame, state, area),
        Tab::Search => render_search(frame, state, area),
    }
}

// ── Overview ──────────────────────────────────────────────────────────────────

fn render_overview(frame: &mut Frame, state: &AppState, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    render_stats_panel(frame, state.store.stats(), cols[0]);
    render_policy_panel(frame, state.store.policy(), cols[1]);
}

fn render_stats_panel(frame: &mut Frame, stats: &OptimizationStats, area: Rect) {
    let total = stats.total_ingested.max(1) as f32;
    let stored_pct = stats.total_stored as f32 / total * 100.0;
    let upgraded_pct = stats.total_upgraded as f32 / total * 100.0;
    let skipped_pct = stats.total_skipped as f32 / total * 100.0;

    let text = format!(
        "\n  Ingested  : {:>6}\n  Stored    : {:>6}  ({stored_pct:.1}%)\n  Upgraded  : {:>6}  ({upgraded_pct:.1}%)\n  Skipped   : {:>6}  ({skipped_pct:.1}%)\n\n  Last ID   : {}",
        stats.total_ingested,
        stats.total_stored,
        stats.total_upgraded,
        stats.total_skipped,
        stats.last_ingest_id.as_deref().unwrap_or("-"),
    );

    let bar_w = (area.width as usize).saturating_sub(6).clamp(8, 30);
    let store_bar = progress_bar(stored_pct / 100.0, bar_w);
    let upg_bar = progress_bar(upgraded_pct / 100.0, bar_w);
    let skip_bar = progress_bar(skipped_pct / 100.0, bar_w);

    let bars =
        format!("\n\n  Stored    {store_bar}\n  Upgraded  {upg_bar}\n  Skipped   {skip_bar}",);

    frame.render_widget(
        Paragraph::new(format!("{text}{bars}"))
            .block(Block::default().borders(Borders::ALL).title(" Statistics "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_policy_panel(frame: &mut Frame, policy: &DecisionPolicy, area: Rect) {
    let text = format!(
        "\n  unique_threshold    : {:.3}\n  duplicate_threshold : {:.3}\n\n  weight_tag    : {:.3}\n  weight_embed  : {:.3}\n  weight_text   : {:.3}\n\n  upgrade_gate.min_embed_cosine : {:.3}\n  upgrade_gate.min_tag_jaccard  : {:.3}",
        policy.unique_threshold,
        policy.duplicate_threshold,
        policy.weight_tag,
        policy.weight_embed,
        policy.weight_text,
        policy.upgrade_gate.min_embed_cosine,
        policy.upgrade_gate.min_tag_jaccard,
    );
    frame.render_widget(
        Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Decision Policy "),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

// ── Memories ──────────────────────────────────────────────────────────────────

fn render_memories(frame: &mut Frame, state: &AppState, area: Rect) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(area);

    render_memory_list(frame, state, cols[0]);
    render_memory_detail_panel(frame, state, cols[1]);
}

fn render_memory_list(frame: &mut Frame, state: &AppState, area: Rect) {
    let memories = state.store.list();
    let items: Vec<ListItem> = memories
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let is_sel = i == state.memory_list_idx;
            let prefix = if is_sel { "▶ " } else { "  " };
            let ver_tag = format!("v{}", m.version);
            let text = format!(
                "{}{:<14} {} rc={} src={}",
                prefix,
                truncate(&m.id, 14),
                ver_tag,
                m.recall_count,
                m.source_count
            );
            let style = if is_sel {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Gray)
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let title = format!(" Memories ({}) ", memories.len());
    let mut list_state = ListState::default();
    list_state.select(Some(state.memory_list_idx));
    frame.render_stateful_widget(
        List::new(items)
            .block(Block::default().borders(Borders::ALL).title(title))
            .highlight_style(Style::default().fg(Color::Yellow)),
        area,
        &mut list_state,
    );
}

fn render_memory_detail_panel(frame: &mut Frame, state: &AppState, area: Rect) {
    let content = if let Some(m) = state.selected_memory() {
        format_memory_detail(m)
    } else {
        "  (no selection)".to_string()
    };
    frame.render_widget(
        Paragraph::new(content)
            .block(Block::default().borders(Borders::ALL).title(" Detail "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn format_memory_detail(m: &GeneralizedMemory) -> String {
    let tags = if m.abstract_tags.is_empty() {
        "(none)".to_string()
    } else {
        m.abstract_tags.join(", ")
    };
    let embed_preview = if m.centroid_embedding.is_empty() {
        "(none)".to_string()
    } else {
        let preview: Vec<String> = m
            .centroid_embedding
            .iter()
            .take(4)
            .map(|v| format!("{v:.3}"))
            .collect();
        let suffix = if m.centroid_embedding.len() > 4 {
            "…"
        } else {
            ""
        };
        format!("[{}{}]", preview.join(", "), suffix)
    };
    format!(
        "\n  ID      : {}\n  Version : v{}\n  Sources : {}\n  Recalls : {}\n\n  Summary :\n  {}\n\n  Tags    : {}\n\n  Embed   : {}\n\n  Created : {}\n  Updated : {}",
        m.id,
        m.version,
        m.source_count,
        m.recall_count,
        m.summary,
        tags,
        embed_preview,
        fmt_epoch(m.created_epoch),
        fmt_epoch(m.last_upgraded_epoch),
    )
}

// ── Audit Log ─────────────────────────────────────────────────────────────────

fn render_audit(frame: &mut Frame, state: &AppState, area: Rect) {
    let log = state.store.audit_log();
    let items: Vec<ListItem> = log
        .iter()
        .skip(state.audit_scroll)
        .map(|entry| {
            let (tag, color) = match &entry.result {
                IngestResult::Stored(_) => ("STORED  ", Color::Green),
                IngestResult::Upgraded { version, .. } => {
                    let _ = version;
                    ("UPGRADED", Color::Cyan)
                }
                IngestResult::Skipped { similarity, .. } => {
                    let _ = similarity;
                    ("SKIPPED ", Color::DarkGray)
                }
            };
            let reason_short = entry.evidence.reason.split('.').next().unwrap_or("").trim();
            let text = format!(
                " {} │ src={:<12} │ {}",
                tag,
                truncate(&entry.source_id, 12),
                truncate(reason_short, 55),
            );
            ListItem::new(text).style(Style::default().fg(color))
        })
        .collect();

    let title = format!(" Audit Log ({} entries) ", log.len());
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title(title)),
        area,
    );
}

// ── Search ────────────────────────────────────────────────────────────────────

fn render_search(frame: &mut Frame, state: &AppState, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)])
        .split(area);

    // 検索入力フィールド
    let input_style = if state.search_input_mode {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::Gray)
    };
    let cursor = if state.search_input_mode { "█" } else { "" };
    frame.render_widget(
        Paragraph::new(format!("{}{}", state.search_query, cursor)).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Search Query (press / to edit) ")
                .border_style(input_style),
        ),
        rows[0],
    );

    // 検索結果リスト
    let items: Vec<ListItem> = state
        .search_results
        .iter()
        .enumerate()
        .map(|(i, (id, score))| {
            let is_sel = i == state.search_list_idx;
            let prefix = if is_sel { "▶ " } else { "  " };
            let mem = state.store.get_by_id(id);
            let summary = mem.map(|m| m.summary.as_str()).unwrap_or("-");
            let text = format!(
                "{}{:<16} {:.3}  {}",
                prefix,
                truncate(id, 16),
                score,
                truncate(summary, 50)
            );
            let style = if is_sel {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            ListItem::new(text).style(style)
        })
        .collect();

    let title = format!(" Results ({}) ", state.search_results.len());
    let mut list_state = ListState::default();
    list_state.select(if state.search_results.is_empty() {
        None
    } else {
        Some(state.search_list_idx)
    });
    frame.render_stateful_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title(title)),
        rows[1],
        &mut list_state,
    );
}

// ── Modal ─────────────────────────────────────────────────────────────────────

fn render_modal(frame: &mut Frame, state: &AppState, area: Rect) {
    match state.modal {
        ModalDialog::PruneConfirm => render_prune_modal(frame, state, area),
        ModalDialog::MemoryDetail => render_detail_modal(frame, state, area),
        ModalDialog::None => {}
    }
}

fn render_prune_modal(frame: &mut Frame, state: &AppState, area: Rect) {
    let stale_count = state
        .store
        .list()
        .iter()
        .filter(|m| m.version == 1 && m.recall_count == 0)
        .count();

    let text = format!(
        "\n  Stale memories to prune: {stale_count}\n  (version=1, recall_count=0)\n\n  [r] Execute prune    [Esc] Cancel",
    );
    let modal_area = centered_rect(50, 40, area);
    frame.render_widget(Clear, modal_area);
    frame.render_widget(
        Paragraph::new(text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Prune Stale Memories ")
                    .border_style(Style::default().fg(Color::Yellow)),
            )
            .wrap(Wrap { trim: false }),
        modal_area,
    );
}

fn render_detail_modal(frame: &mut Frame, state: &AppState, area: Rect) {
    let content = if let Some(id) = &state.detail_memory_id {
        if let Some(m) = state.store.get_by_id(id) {
            format_memory_detail(m)
        } else {
            "  (not found)".to_string()
        }
    } else {
        "  (no selection)".to_string()
    };

    let modal_area = centered_rect(70, 70, area);
    frame.render_widget(Clear, modal_area);
    frame.render_widget(
        Paragraph::new(content)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Memory Detail [Esc] close ")
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: false }),
        modal_area,
    );
}

// ── Status bar ────────────────────────────────────────────────────────────────

fn render_status_bar(frame: &mut Frame, state: &AppState, area: Rect) {
    let save_hint = if state.store_path.is_some() {
        "s save   "
    } else {
        ""
    };
    let dirty_marker = if state.dirty { "[*] " } else { "" };
    let path_label = match &state.store_path {
        Some(p) => format!(" [{}]", p.display()),
        None => " [no file]".to_string(),
    };
    let help =
        format!("  Tab switch   ↑↓ nav   / search   Enter detail   {save_hint}p prune   q quit");
    let left_raw = format!("{dirty_marker}{}{path_label}", state.status_msg);
    let max_left = (area.width as usize).saturating_sub(help.len());
    let left = truncate(&left_raw, max_left);
    let pad = " ".repeat(max_left.saturating_sub(left.len()));
    let text = format!("{left}{pad}{help}");
    let style = if state.dirty {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    frame.render_widget(Paragraph::new(text).style(style), area);
}

// ── Event loop ────────────────────────────────────────────────────────────────

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
) -> Result<(), String> {
    loop {
        terminal
            .draw(|frame| render(frame, state))
            .map_err(|e| e.to_string())?;

        if event::poll(Duration::from_millis(50)).map_err(|e| e.to_string())?
            && let Event::Key(key) = event::read().map_err(|e| e.to_string())?
            && key.kind == event::KeyEventKind::Press
            && handle_key(state, key)
        {
            break;
        }
    }
    Ok(())
}

// ── Demo data ─────────────────────────────────────────────────────────────────

fn build_demo_store() -> PersistentMemoryStore {
    let mut store = PersistentMemoryStore::new();

    let records: &[(&str, &str, &[&str], &[f32])] = &[
        (
            "r001",
            "Design a RESTful API with JWT authentication",
            &["api", "rest", "jwt"],
            &[0.9, 0.1, 0.0, 0.0],
        ),
        (
            "r002",
            "Add rate limiting to REST API endpoints",
            &["api", "rest", "rate-limit"],
            &[0.88, 0.12, 0.0, 0.0],
        ),
        (
            "r003",
            "Implement OAuth2 flow for REST API",
            &["api", "rest", "oauth2", "auth"],
            &[0.85, 0.15, 0.0, 0.0],
        ),
        (
            "r004",
            "Design a PostgreSQL schema for user management",
            &["db", "sql", "postgres", "users"],
            &[0.0, 0.0, 0.9, 0.1],
        ),
        (
            "r005",
            "Add indexes to PostgreSQL tables for performance",
            &["db", "sql", "postgres", "index"],
            &[0.0, 0.0, 0.88, 0.12],
        ),
        (
            "r006",
            "Design a React component library with Storybook",
            &["frontend", "react", "ui", "storybook"],
            &[0.0, 0.8, 0.0, 0.2],
        ),
        (
            "r007",
            "Add TypeScript types to React components",
            &["frontend", "react", "typescript"],
            &[0.0, 0.82, 0.0, 0.18],
        ),
        (
            "r008",
            "Deploy microservices to Kubernetes",
            &["infra", "k8s", "deploy", "microservice"],
            &[0.5, 0.0, 0.0, 0.5],
        ),
        (
            "r009",
            "Completely different concept about ML pipelines",
            &["ml", "pipeline", "training"],
            &[0.0, 0.0, 0.0, 1.0],
        ),
        (
            "r010",
            "Another ML concept: feature engineering",
            &["ml", "features", "preprocessing"],
            &[0.0, 0.0, 0.05, 0.95],
        ),
    ];

    for (id, text, tags, embed) in records {
        let record = MemoryRecord {
            id: id.to_string(),
            text: text.to_string(),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            embedding: Some(embed.to_vec()),
            architecture: None,
            relations: Vec::new(),
        };
        store.ingest(&record);
    }

    store
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

fn progress_bar(value: f32, width: usize) -> String {
    let clamped = value.clamp(0.0, 1.0);
    let filled = ((clamped * width as f32).round() as usize).min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else if max > 1 {
        format!("{}…", &s[..max - 1])
    } else {
        "…".to_string()
    }
}

fn fmt_epoch(epoch: u64) -> String {
    if epoch == 0 {
        return "-".to_string();
    }
    // 簡易表示: epoch 秒を "YYYY-MM-DD HH:MM:SS UTC" 風に変換
    let secs = epoch;
    let days = secs / 86400;
    let rem = secs % 86400;
    let h = rem / 3600;
    let m = (rem % 3600) / 60;
    let s = rem % 60;
    // 1970-01-01 からの日数でグレゴリオ暦の近似値を計算
    let year = 1970 + days / 365;
    let yday = days % 365;
    let month = yday / 30 + 1;
    let day = yday % 30 + 1;
    format!("{year:04}-{month:02}-{day:02} {h:02}:{m:02}:{s:02} UTC")
}

// ── main ─────────────────────────────────────────────────────────────────────

fn main() {
    if let Err(err) = run_with_args(std::env::args_os()) {
        eprintln!("Error: {err}");
        std::process::exit(1);
    }
}

pub fn run_with_args<I, T>(args: I) -> Result<(), String>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let args = match Args::try_parse_from(args) {
        Ok(args) => args,
        Err(err) => match err.kind() {
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => {
                print!("{err}");
                return Ok(());
            }
            _ => return Err(err.to_string()),
        },
    };
    run_app(args)
}

fn run_app(args: Args) -> Result<(), String> {
    // ストアの初期化: --store が指定されていればファイルからロード
    let store = match &args.store {
        Some(path) if !args.demo => match PersistentMemoryStore::load_or_new(path) {
            Ok(s) => {
                eprintln!(
                    "memory_admin: {} memories loaded from {}.",
                    s.memory_count(),
                    path.display()
                );
                s
            }
            Err(e) => return Err(format!("Failed to load store from {}: {e}", path.display())),
        },
        _ => {
            if args.demo {
                eprintln!("Loading demo data...");
            }
            let s = if args.demo {
                build_demo_store()
            } else {
                PersistentMemoryStore::new()
            };
            eprintln!("memory_admin: {} memories in store.", s.memory_count());
            s
        }
    };

    if let Some(command) = args.command {
        return run_memory_command(command, &store);
    }

    enable_raw_mode().expect("Failed to enable raw mode");
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)
        .expect("Failed to enter alternate screen");

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend).expect("Failed to create terminal");

    let mut state = AppState::new(store, args.store.clone());

    let result = run_event_loop(&mut terminal, &mut state);

    disable_raw_mode().ok();
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )
    .ok();
    terminal.show_cursor().ok();

    // --store 指定かつ未保存の変更がある場合は自動保存
    if state.dirty
        && let Some(path) = &state.store_path
    {
        match state.store.save(path) {
            Ok(()) => eprintln!("Auto-saved to {}.", path.display()),
            Err(e) => eprintln!("Auto-save failed: {e}"),
        }
    }

    result?;
    Ok(())
}

fn run_memory_command(command: MemoryCommand, store: &PersistentMemoryStore) -> Result<(), String> {
    match command {
        MemoryCommand::Rewrite {
            preview: _,
            validate,
            apply,
            yes,
        } => run_memory_rewrite(store, validate, apply, yes),
        MemoryCommand::Rollback => run_memory_rollback(store),
        MemoryCommand::Topology => run_memory_topology(store),
        MemoryCommand::Drift => run_memory_drift(store),
        MemoryCommand::Attractors => run_memory_attractors(store),
    }
}

fn run_memory_rewrite(
    store: &PersistentMemoryStore,
    validate: bool,
    apply: bool,
    yes: bool,
) -> Result<(), String> {
    let graph = semantic_graph_from_store(store);
    let transaction = semantic_rewrite_transaction(&graph);

    if apply && !yes {
        return Err("memory rewrite --apply requires operator confirmation via --yes".to_string());
    }

    if apply {
        let request = crate::runtime::semantic::RuntimeSemanticApplyRequest {
            validation: transaction.validation.clone(),
            runtime_checksum: transaction.deterministic_checksum,
            transaction,
            apply_mode: crate::runtime::semantic::SemanticApplyMode::Strict,
        };
        let result = crate::runtime::semantic::runtime_semantic_apply(request);
        println!(
            "semantic apply: applied={} topology_updated={} rollback_available={} checksum={} revision={}",
            result.applied,
            result.topology_updated,
            result.rollback_available,
            result.applied_checksum,
            result.topology_revision
        );
        for warning in result.warnings {
            println!("warning: {warning}");
        }
        return Ok(());
    }

    if validate {
        println!(
            "semantic validation: valid={} continuity={} anchors={} contradiction={} mass={} replay={} topology={}",
            transaction.validation.valid,
            transaction.validation.continuity_retained,
            transaction.validation.anchors_preserved,
            transaction.validation.contradiction_bounded,
            transaction.validation.semantic_mass_bounded,
            transaction.validation.replay_invariant,
            transaction.validation.topology_invariant
        );
        for error in transaction.validation.validation_errors {
            println!("validation_error: {error}");
        }
        return Ok(());
    }

    println!(
        "semantic preview: identities={} merge_ops={} correction_ops={} compression_ops={} continuity_delta={:.6} mass_delta={:.6} contradiction_delta={:.6} anchor_preservation={:.6} checksum={}",
        graph.identities.len(),
        transaction.rewrite_plan.merge_operations.len(),
        transaction.rewrite_plan.correction_operations.len(),
        transaction.rewrite_plan.compression_operations.len(),
        transaction.preview.continuity_delta,
        transaction.preview.semantic_mass_delta,
        transaction.preview.contradiction_delta,
        transaction.preview.anchor_preservation_ratio,
        transaction.deterministic_checksum
    );
    Ok(())
}

fn run_memory_rollback(store: &PersistentMemoryStore) -> Result<(), String> {
    let graph = semantic_graph_from_store(store);
    let snapshot = semantic_rollback_snapshot(&graph);
    let result = crate::runtime::semantic::runtime_semantic_rollback(snapshot);
    println!(
        "semantic rollback: restored={} revision={} replay_invariant_retained={}",
        result.restored, result.restored_revision, result.replay_invariant_retained
    );
    Ok(())
}

fn run_memory_topology(store: &PersistentMemoryStore) -> Result<(), String> {
    let graph = semantic_graph_from_store(store);
    let snapshot = crate::runtime::unified_projection::semantic_runtime_snapshot(&graph);
    println!(
        "semantic topology: identities={} lineages={} stabilizations={}",
        snapshot.topology_snapshot.identities.len(),
        snapshot.lineage_snapshot.len(),
        snapshot.stabilization_snapshot.len()
    );
    for identity in snapshot.topology_snapshot.identities {
        println!(
            "identity: id={} continuity={:.6} invariant_core_overlap={:.6} drift_lineage={:?}",
            identity.identity_id,
            identity.continuity_score,
            identity.invariant_core_overlap,
            identity.drift_lineage
        );
    }
    Ok(())
}

fn run_memory_drift(store: &PersistentMemoryStore) -> Result<(), String> {
    let graph = semantic_graph_from_store(store);
    let snapshot = crate::runtime::unified_projection::semantic_runtime_snapshot(&graph);
    println!("semantic drift: events={}", snapshot.drift_snapshot.len());
    for drift in snapshot.drift_snapshot {
        println!(
            "drift: identity={} previous={:.6} current={:.6} magnitude={:.6} recoverable={}",
            drift.identity_id,
            drift.previous_continuity,
            drift.current_continuity,
            drift.drift_magnitude,
            drift.recoverable
        );
    }
    Ok(())
}

fn run_memory_attractors(store: &PersistentMemoryStore) -> Result<(), String> {
    let graph = semantic_graph_from_store(store);
    let snapshot = crate::runtime::unified_projection::semantic_runtime_snapshot(&graph);
    println!(
        "semantic attractors: attractors={}",
        snapshot.attractor_snapshot.len()
    );
    for attractor in snapshot.attractor_snapshot {
        println!(
            "attractor: id={} anchors={} strength={:.6} mass={:.6} stability={:.6}",
            attractor.attractor_id,
            attractor.anchor_set.len(),
            attractor.attractor_strength,
            attractor.semantic_mass,
            attractor.stability_score
        );
    }
    Ok(())
}

fn semantic_graph_from_store(store: &PersistentMemoryStore) -> SemanticIdentityGraph {
    let mut identities = store
        .list()
        .iter()
        .map(|memory| SemanticIdentityCandidate {
            identity_id: stable_memory_identity(&memory.id),
            continuity_score: 1.0,
            invariant_core_overlap: 1.0,
            drift_lineage: vec![memory.version as u64, memory.source_count as u64],
        })
        .collect::<Vec<_>>();
    identities.sort_by(|a, b| a.identity_id.cmp(&b.identity_id));
    SemanticIdentityGraph { identities }
}

fn stable_memory_identity(value: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
