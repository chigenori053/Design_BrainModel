use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText};

use crate::ir_loader::IrTracker;
use crate::model::{
    DispatchAction, DispatchNl, SourceBinding, SourcePathResolver, StructureViewIR,
    ValidationOverlay, ViewMode,
};
use crate::nl_chat::{LocalCommand, NlChatPanel};

#[derive(Clone)]
pub struct ViewerAppConfig {
    pub mode: ViewMode,
    pub ir_path: PathBuf,
    pub root: PathBuf,
    pub diagnostics: ValidationOverlay,
    pub dispatch_action: DispatchAction,
    pub source_path_for_node: SourcePathResolver,
    pub dispatch_nl: DispatchNl,
}

pub struct ViewerApp {
    config: ViewerAppConfig,
    tracker: IrTracker,
    ir: StructureViewIR,
    mode: ViewMode,
    selected_node: Option<String>,
    search: String,
    status: String,
    last_reload: Instant,
    animation_start: Instant,
    nl_chat: NlChatPanel,
    show_node_popup: bool,
}

impl ViewerApp {
    pub fn new(cc: &eframe::CreationContext<'_>, config: ViewerAppConfig) -> Self {
        // macOS のシステムフォントから日本語グリフをロードする（文字化け対策）
        setup_cjk_font(&cc.egui_ctx);
        let mut tracker = IrTracker::new(config.ir_path.clone());
        let ir = tracker
            .load_initial()
            .unwrap_or_else(|_err| StructureViewIR::default());
        let selected_node = ir.selection.selected_nodes.first().cloned();
        let mode = config.mode;
        Self {
            config,
            tracker,
            ir,
            mode,
            selected_node,
            search: String::new(),
            status: String::new(),
            last_reload: Instant::now(),
            animation_start: Instant::now(),
            nl_chat: NlChatPanel::new(),
            show_node_popup: false,
        }
    }

    fn reload_ir(&mut self, force: bool) {
        let result = if force {
            self.tracker.load_initial().map(Some)
        } else {
            self.tracker.reload_if_changed()
        };
        match result {
            Ok(Some(ir)) => {
                self.ir = ir;
                if self.selected_node.is_none() {
                    self.selected_node = self.ir.selection.selected_nodes.first().cloned();
                }
                self.last_reload = Instant::now();
                self.animation_start = Instant::now();
            }
            Ok(None) => {}
            Err(err) => {
                self.status = err;
            }
        }
    }

    fn source_binding(&self) -> Option<SourceBinding> {
        let node = self.selected_node.as_ref()?;
        self.ir
            .scene_3d
            .as_ref()
            .and_then(|scene| scene.graph.nodes.iter().find(|item| item.id == *node))
            .and_then(|item| item.source_binding.clone())
            .or_else(|| {
                (self.config.source_path_for_node)(node).map(|path| SourceBinding {
                    file: path,
                    line_start: 1,
                    line_end: 1,
                    symbol: Some(node.clone()),
                })
            })
    }

    /// ステータスバー: サイクル数・違反数・ノード/エッジ数を表示
    fn render_status_bar(&self, ui: &mut egui::Ui) {
        let cycle_count = self.ir.edges.iter().filter(|e| e.cycle).count();
        let violation_count = self
            .ir
            .risk_overlay
            .iter()
            .filter(|r| r.level == "error" || r.level == "violation")
            .count();

        ui.horizontal(|ui| {
            ui.label(
                RichText::new(format!("nodes: {}", self.ir.nodes.len()))
                    .color(Color32::from_rgb(80, 100, 80))
                    .small(),
            );
            ui.separator();
            ui.label(
                RichText::new(format!("edges: {}", self.ir.edges.len()))
                    .color(Color32::from_rgb(80, 100, 80))
                    .small(),
            );
            ui.separator();
            let cycle_color = if cycle_count > 0 {
                Color32::from_rgb(196, 73, 61)
            } else {
                Color32::from_rgb(80, 140, 80)
            };
            ui.label(
                RichText::new(format!("cycles: {cycle_count}"))
                    .color(cycle_color)
                    .small(),
            );
            ui.separator();
            let viol_color = if violation_count > 0 {
                Color32::from_rgb(200, 130, 40)
            } else {
                Color32::from_rgb(80, 140, 80)
            };
            ui.label(
                RichText::new(format!("violations: {violation_count}"))
                    .color(viol_color)
                    .small(),
            );
            if !self.status.is_empty() {
                ui.separator();
                ui.label(
                    RichText::new(&self.status)
                        .color(Color32::from_rgb(130, 130, 130))
                        .small(),
                );
            }
        });
    }
}

impl eframe::App for ViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.reload_ir(false);
        ctx.request_repaint_after(Duration::from_millis(250));

        // ── トップバー（最小限） ────────────────────────────────
        egui::TopBottomPanel::top("topbar")
            .exact_height(36.0)
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.heading(
                        RichText::new("DBM Viewer")
                            .size(15.0)
                            .color(Color32::from_rgb(40, 60, 100)),
                    );
                    ui.separator();

                    // 2D / 3D モード切替
                    if ui
                        .selectable_label(matches!(self.mode, ViewMode::TwoD), "2D")
                        .clicked()
                    {
                        self.mode = ViewMode::TwoD;
                    }
                    if ui
                        .selectable_label(matches!(self.mode, ViewMode::ThreeD), "3D")
                        .clicked()
                    {
                        self.mode = ViewMode::ThreeD;
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let cycle_count = self.ir.edges.iter().filter(|e| e.cycle).count();
                        let viol_count = self.config.diagnostics.layer_violations.max(
                            self.ir
                                .risk_overlay
                                .iter()
                                .filter(|r| r.level == "error")
                                .count(),
                        );

                        if cycle_count > 0 {
                            ui.label(
                                RichText::new(format!("⚠ {cycle_count} cycles"))
                                    .color(Color32::from_rgb(196, 73, 61))
                                    .small(),
                            );
                        }
                        if viol_count > 0 {
                            ui.label(
                                RichText::new(format!("  {viol_count} violations"))
                                    .color(Color32::from_rgb(200, 130, 40))
                                    .small(),
                            );
                        }
                        if cycle_count == 0 && viol_count == 0 {
                            ui.label(
                                RichText::new("✓ clean")
                                    .color(Color32::from_rgb(80, 160, 80))
                                    .small(),
                            );
                        }
                    });
                });
            });

        // ── ステータスバー（下部） ─────────────────────────────
        egui::TopBottomPanel::bottom("statusbar")
            .exact_height(22.0)
            .show(ctx, |ui| {
                self.render_status_bar(ui);
            });

        // ── NLチャットパネル（右・リサイズ可） ─────────────────
        egui::SidePanel::right("nl_chat")
            .resizable(true)
            .default_width(320.0)
            .min_width(200.0)
            .max_width(640.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Natural Language")
                        .size(13.0)
                        .color(Color32::from_rgb(60, 80, 120)),
                );
                ui.separator();

                let dispatch_nl = self.config.dispatch_nl.clone();
                let local_cmd =
                    self.nl_chat
                        .render(ui, self.selected_node.as_deref(), &dispatch_nl);

                if let Some(cmd) = local_cmd {
                    match cmd {
                        LocalCommand::SwitchMode2D => self.mode = ViewMode::TwoD,
                        LocalCommand::SwitchMode3D => self.mode = ViewMode::ThreeD,
                        LocalCommand::Search(term) => self.search = term,
                    }
                }
            });

        // ── メインマップ（CentralPanel） ───────────────────────
        egui::CentralPanel::default().show(ctx, |ui| {
            if matches!(self.mode, ViewMode::ThreeD) {
                crate::space_3d::render(ui, &self.ir, &mut self.selected_node);
            } else {
                crate::graph_2d::render(ui, &self.ir, &self.search, &mut self.selected_node);
            }

            if self.selected_node.is_some() {
                self.show_node_popup = true;
            }
        });

        // ── ノードポップアップ ─────────────────────────────────
        if self.show_node_popup {
            // render_node_popup は ctx への参照が必要なため egui::Window を直接使う
            let node_id = self.selected_node.clone();
            if let Some(node_id) = node_id
                && let Some(node) = self
                    .ir
                    .nodes
                    .iter()
                    .find(|n| n.id == node_id || n.label == node_id)
                    .cloned()
            {
                let incoming = self.ir.edges.iter().filter(|e| e.to == node.id).count();
                let outgoing = self.ir.edges.iter().filter(|e| e.from == node.id).count();
                let cycles = self
                    .ir
                    .edges
                    .iter()
                    .filter(|e| (e.from == node.id || e.to == node.id) && e.cycle)
                    .count();
                let source_binding = self.source_binding();
                let mut close = false;
                let mut open_src = false;

                egui::Window::new(format!("● {}", node.label))
                    .id(egui::Id::new("node_popup"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-340.0, 42.0))
                    .show(ctx, |ui| {
                        ui.label(
                            RichText::new(format!("layer: {}  role: {}", node.layer, node.role))
                                .small()
                                .color(Color32::GRAY),
                        );
                        ui.label(format!("in: {incoming}  out: {outgoing}"));
                        if cycles > 0 {
                            ui.label(
                                RichText::new(format!("⚠ cycles: {cycles}"))
                                    .color(Color32::from_rgb(196, 73, 61)),
                            );
                        }
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if source_binding.is_some() && ui.small_button("Source").clicked() {
                                open_src = true;
                            }
                            if ui.small_button("✕").clicked() {
                                close = true;
                            }
                        });
                    });

                if open_src
                    && let Some(binding) = source_binding
                    && let Err(e) =
                        crate::source_jump::open_source(&binding, Some(&self.config.root))
                {
                    self.status = e;
                }
                if close {
                    self.selected_node = None;
                    self.show_node_popup = false;
                }
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// CJK フォントセットアップ（文字化け対策）
// ──────────────────────────────────────────────────────────────────────────────

/// システムフォントから日本語グリフを egui に追加する。
/// フォントが見つからない場合は何もしない（graceful fallback）。
fn setup_cjk_font(ctx: &egui::Context) {
    let font_bytes = load_system_cjk_font();
    let Some(bytes) = font_bytes else { return };

    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("cjk_system".to_owned(), egui::FontData::from_owned(bytes));

    // Proportional フォントの末尾に追加（既存フォントがグリフを持たない場合のフォールバック）
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .push("cjk_system".to_owned());

    ctx.set_fonts(fonts);
}

/// OS ごとのシステム CJK フォントパスを試してバイト列を返す
fn load_system_cjk_font() -> Option<Vec<u8>> {
    let candidates: &[&str] = &[
        // macOS
        "/System/Library/Fonts/ヒラギノ角ゴシック W4.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/Library/Fonts/Arial Unicode MS.ttf",
        // Linux
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJKjp-Regular.otf",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        // Windows
        "C:/Windows/Fonts/msgothic.ttc",
        "C:/Windows/Fonts/meiryo.ttc",
    ];
    for path in candidates {
        if let Ok(bytes) = std::fs::read(path) {
            return Some(bytes);
        }
    }
    None
}
