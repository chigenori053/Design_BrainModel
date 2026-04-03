use std::path::PathBuf;
use std::time::{Duration, Instant};

use design_cli::viewer::{ViewMode, StructureViewIR};
use eframe::egui::{self, Color32, RichText};

use crate::ipc::{ActionRequest, dispatch_action, write_action};
use crate::ir_loader::IrTracker;
use crate::nl_chat::{LocalCommand, NlChatPanel};

#[derive(Debug, Clone)]
pub struct ViewerAppConfig {
    pub mode: ViewMode,
    pub ir_path: PathBuf,
    pub root: PathBuf,
    pub cli_path: Option<PathBuf>,
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
    pub fn new(_cc: &eframe::CreationContext<'_>, config: ViewerAppConfig) -> Self {
        let mut tracker = IrTracker::new(config.ir_path.clone());
        let ir = tracker
            .load_initial()
            .unwrap_or_else(|_err| StructureViewIR {
                version: 2,
                nodes: Vec::new(),
                edges: Vec::new(),
                preview: None,
                snapshots: Vec::new(),
                history: Vec::new(),
                risk_overlay: Vec::new(),
                selection: Default::default(),
                candidates: Vec::new(),
                heatmap: Vec::new(),
                design_sync: Default::default(),
            });
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

    fn source_path(&self) -> Option<PathBuf> {
        let node = self.selected_node.as_ref()?;
        let direct = self.config.root.join("src").join(format!("{node}.rs"));
        if direct.exists() {
            return Some(direct);
        }
        let module = self.config.root.join("src").join(node).join("mod.rs");
        if module.exists() {
            return Some(module);
        }
        None
    }

    fn open_source(path: &std::path::Path) -> Result<(), String> {
        let status = if cfg!(target_os = "macos") {
            std::process::Command::new("open").arg(path).status()
        } else if cfg!(target_os = "windows") {
            std::process::Command::new("cmd")
                .args(["/C", "start", "", &path.display().to_string()])
                .status()
        } else {
            std::process::Command::new("xdg-open").arg(path).status()
        }
        .map_err(|err| format!("failed to open {}: {err}", path.display()))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("source jump failed for {}", path.display()))
        }
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
                        let viol_count = self
                            .ir
                            .risk_overlay
                            .iter()
                            .filter(|r| r.level == "error" || r.level == "violation")
                            .count();

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
                let cycle_count = self.ir.edges.iter().filter(|e| e.cycle).count();
                let viol_count = self
                    .ir
                    .risk_overlay
                    .iter()
                    .filter(|r| r.level == "error")
                    .count();
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("nodes: {}", self.ir.nodes.len()))
                            .small()
                            .color(Color32::from_rgb(80, 100, 80)),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!("edges: {}", self.ir.edges.len()))
                            .small()
                            .color(Color32::from_rgb(80, 100, 80)),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!("cycles: {cycle_count}"))
                            .small()
                            .color(if cycle_count > 0 {
                                Color32::from_rgb(196, 73, 61)
                            } else {
                                Color32::from_rgb(80, 140, 80)
                            }),
                    );
                    ui.separator();
                    ui.label(
                        RichText::new(format!("violations: {viol_count}"))
                            .small()
                            .color(if viol_count > 0 {
                                Color32::from_rgb(200, 130, 40)
                            } else {
                                Color32::from_rgb(80, 140, 80)
                            }),
                    );
                    if !self.status.is_empty() {
                        ui.separator();
                        ui.label(
                            RichText::new(&self.status)
                                .small()
                                .color(Color32::from_rgb(130, 130, 130)),
                        );
                    }
                });
            });

        // ── NLチャットパネル（右固定） ─────────────────────────
        egui::SidePanel::right("nl_chat")
            .resizable(false)
            .exact_width(320.0)
            .show(ctx, |ui| {
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Natural Language")
                        .size(13.0)
                        .color(Color32::from_rgb(60, 80, 120)),
                );
                ui.separator();

                let root = self.config.root.clone();
                let local_cmd = self.nl_chat.render(
                    ui,
                    self.selected_node.as_deref(),
                    &root,
                );

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
                viewer_core::space_3d::render(ui, &self.ir.into_core(), &mut self.selected_node);
            } else {
                viewer_core::graph_2d::render(ui, &self.ir.into_core(), &self.search, &mut self.selected_node);
            }
            if self.selected_node.is_some() {
                self.show_node_popup = true;
            }
        });

        // ── ノードポップアップ ─────────────────────────────────
        if self.show_node_popup {
            if let Some(node_id) = self.selected_node.clone() {
                if let Some(node) = self
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
                    let source_path = self.source_path();
                    let mut close = false;
                    let mut open_src = false;

                    egui::Window::new(format!("● {}", node.label))
                        .id(egui::Id::new("node_popup"))
                        .collapsible(false)
                        .resizable(false)
                        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-340.0, 42.0))
                        .show(ctx, |ui| {
                            ui.label(
                                RichText::new(format!(
                                    "layer: {}  role: {}",
                                    node.layer, node.role
                                ))
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
                                if source_path.is_some() && ui.small_button("Source").clicked() {
                                    open_src = true;
                                }
                                if ui.small_button("✕").clicked() {
                                    close = true;
                                }
                            });
                        });

                    if open_src {
                        if let Some(path) = source_path {
                            if let Err(e) = Self::open_source(&path) {
                                self.status = e;
                            }
                        }
                    }
                    if close {
                        self.selected_node = None;
                        self.show_node_popup = false;
                    }
                }
            }
        }
    }
}
