use std::fs;
use std::path::{Path, PathBuf};

use eframe::egui;
use serde::Deserialize;

fn main() -> eframe::Result<()> {
    let input = std::env::args().nth(1).map(PathBuf::from);
    let startup_report = if let Some(path) = input {
        match load_report_from_path(&path) {
            Ok(report) => Some(report),
            Err(err) => {
                eprintln!("failed to load report at startup: {err}");
                std::process::exit(1);
            }
        }
    } else {
        None
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 860.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Objective Evaluation Dashboard v1.1",
        native_options,
        Box::new(move |_cc| Ok(Box::new(EvalDashboardApp::new(startup_report.clone())))),
    )
}

#[derive(Debug, Clone, Deserialize)]
struct EvalReport {
    #[serde(rename = "objective_vector_spec_status")]
    spec_status: String,
    frontier_size: usize,
    frontier_hypervolume: f64,
    #[serde(rename = "objective_correlation_matrix")]
    correlation_matrix: [[f64; 4]; 4],
    #[serde(rename = "frontier_objective_mean")]
    frontier_mean: [f64; 4],
    #[serde(rename = "frontier_objective_variance")]
    frontier_variance: [f64; 4],
    cases: Vec<CaseData>,
}

#[derive(Debug, Clone, Deserialize)]
struct CaseData {
    case_id: String,
    raw: [f64; 4],
    normalized: [f64; 4],
    clamped: [f64; 4],
    domination_count: usize,
    pareto_rank: usize,
}

#[derive(Default)]
struct AppState {
    report: Option<EvalReport>,
    selected_case_index: Option<usize>,
    scatter_x: usize,
    scatter_y: usize,
    last_loaded_path: Option<PathBuf>,
    load_path_input: String,
    error_message: Option<String>,
    error_open: bool,
}

struct EvalDashboardApp {
    state: AppState,
}

impl EvalDashboardApp {
    fn new(report: Option<EvalReport>) -> Self {
        let selected_case_index = report
            .as_ref()
            .and_then(|r| if r.cases.is_empty() { None } else { Some(0) });
        Self {
            state: AppState {
                report,
                selected_case_index,
                scatter_x: 0,
                scatter_y: 1,
                last_loaded_path: None,
                load_path_input: String::new(),
                error_message: None,
                error_open: false,
            },
        }
    }

    fn set_error(&mut self, message: impl Into<String>) {
        self.state.error_message = Some(message.into());
        self.state.error_open = true;
    }

    fn load_report_action(&mut self, path: PathBuf) {
        match load_report_from_path(&path) {
            Ok(report) => {
                self.state.selected_case_index = if report.cases.is_empty() {
                    None
                } else {
                    Some(0)
                };
                self.state.last_loaded_path = Some(path);
                self.state.report = Some(report);
            }
            Err(err) => self.set_error(err),
        }
    }
}

impl eframe::App for EvalDashboardApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let dropped_path = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped_path {
            self.load_report_action(path);
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("JSON Path:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.state.load_path_input)
                        .hint_text("/path/to/phase1_eval.json"),
                );
                if ui.button("Load").clicked() {
                    let path = PathBuf::from(self.state.load_path_input.trim());
                    if path.as_os_str().is_empty() {
                        self.set_error("path is empty");
                    } else {
                        self.load_report_action(path);
                    }
                }
                if let Some(path) = &self.state.last_loaded_path {
                    ui.weak(path.display().to_string());
                }
                ui.separator();
                ui.weak("or drag & drop a JSON file");
            });
        });

        if self.state.error_open {
            let mut open = self.state.error_open;
            egui::Window::new("Load Error")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    if let Some(msg) = &self.state.error_message {
                        ui.colored_label(egui::Color32::RED, msg);
                    }
                    if ui.button("Close").clicked() {
                        self.state.error_open = false;
                    }
                });
            self.state.error_open = open;
        }

        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(report) = self.state.report.clone() else {
                ui.centered_and_justified(|ui| {
                    ui.heading("Load a phase1-eval JSON file to start");
                });
                return;
            };
            let selected_case = self
                .state
                .selected_case_index
                .and_then(|idx| report.cases.get(idx))
                .cloned();

            render_summary_panel(ui, &report);
            ui.add_space(8.0);

            ui.columns(2, |columns| {
                render_radar_panel(
                    &mut columns[0],
                    &report,
                    selected_case.as_ref(),
                    self.state.selected_case_index,
                );
                render_scatter_panel(
                    &mut columns[1],
                    &report,
                    &mut self.state.scatter_x,
                    &mut self.state.scatter_y,
                    self.state.selected_case_index,
                );
            });

            ui.separator();

            ui.columns(2, |columns| {
                render_heatmap_panel(&mut columns[0], &report);
                render_case_drilldown_panel(
                    &mut columns[1],
                    &report,
                    &mut self.state.selected_case_index,
                );
            });
        });
    }
}

fn load_report_from_path(path: &Path) -> Result<EvalReport, String> {
    let content = fs::read_to_string(path)
        .map_err(|e| format!("failed to read file {}: {e}", path.display()))?;
    let report: EvalReport =
        serde_json::from_str(&content).map_err(|e| format!("JSON schema mismatch: {e}"))?;
    validate_report(&report)?;
    Ok(report)
}

fn validate_report(report: &EvalReport) -> Result<(), String> {
    if report.cases.is_empty() {
        return Err("cases must not be empty".to_string());
    }
    for row in report.correlation_matrix {
        for v in row {
            if !v.is_finite() {
                return Err("objective_correlation_matrix contains non-finite values".to_string());
            }
        }
    }
    for (idx, case) in report.cases.iter().enumerate() {
        for v in case.raw {
            if !v.is_finite() {
                return Err(format!("case[{idx}] raw contains non-finite values"));
            }
        }
        for v in case.normalized {
            if !v.is_finite() {
                return Err(format!("case[{idx}] normalized contains non-finite values"));
            }
        }
        for v in case.clamped {
            if !v.is_finite() {
                return Err(format!("case[{idx}] clamped contains non-finite values"));
            }
        }
    }
    Ok(())
}

fn render_summary_panel(ui: &mut egui::Ui, report: &EvalReport) {
    ui.group(|ui| {
        ui.heading("Summary");
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("spec_status: {}", report.spec_status));
            ui.separator();
            ui.label(format!("frontier_size: {}", report.frontier_size));
            ui.separator();
            ui.label(format!(
                "frontier_hypervolume: {:.4}",
                report.frontier_hypervolume
            ));
            ui.separator();
            ui.label(format!("case_count: {}", report.cases.len()));
        });
    });
}

fn render_radar_panel(
    ui: &mut egui::Ui,
    report: &EvalReport,
    selected_case: Option<&CaseData>,
    selected_idx: Option<usize>,
) {
    ui.group(|ui| {
        ui.heading("Radar");
        let desired = egui::vec2(ui.available_width(), 280.0);
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        let center = rect.center();
        let radius = (rect.width().min(rect.height()) * 0.36).max(40.0);

        for ring in [0.25_f32, 0.5, 0.75, 1.0] {
            painter.circle_stroke(
                center,
                radius * ring,
                egui::Stroke::new(1.0, egui::Color32::from_gray(80)),
            );
        }

        let labels = ["O0", "O1", "O2", "O3"];
        for (i, label) in labels.iter().enumerate() {
            let angle = std::f32::consts::TAU * (i as f32) / 4.0 - std::f32::consts::FRAC_PI_2;
            let dir = egui::vec2(angle.cos(), angle.sin());
            painter.line_segment(
                [center, center + dir * radius],
                egui::Stroke::new(1.0, egui::Color32::from_gray(90)),
            );
            painter.text(
                center + dir * (radius + 14.0),
                egui::Align2::CENTER_CENTER,
                *label,
                egui::FontId::proportional(12.0),
                egui::Color32::WHITE,
            );
        }

        if let Some(case) = selected_case {
            let mut points = Vec::with_capacity(5);
            for i in 0..4 {
                let v = case.normalized[i].clamp(0.0, 1.0) as f32;
                let angle = std::f32::consts::TAU * (i as f32) / 4.0 - std::f32::consts::FRAC_PI_2;
                let p = center + egui::vec2(angle.cos(), angle.sin()) * (radius * v);
                points.push(p);
            }
            points.push(points[0]);
            let is_front = case.pareto_rank == 1;
            let fill = if is_front {
                egui::Color32::from_rgba_unmultiplied(255, 90, 80, 110)
            } else {
                egui::Color32::from_rgba_unmultiplied(90, 170, 255, 60)
            };
            let stroke = if is_front {
                egui::Stroke::new(2.2, egui::Color32::from_rgb(255, 110, 95))
            } else {
                egui::Stroke::new(1.6, egui::Color32::from_rgb(120, 190, 255))
            };
            painter.add(egui::Shape::convex_polygon(points.clone(), fill, stroke));
            painter.add(egui::Shape::line(points, stroke));
        }

        let frontier_count = report.cases.iter().filter(|c| c.pareto_rank == 1).count();
        ui.label(format!(
            "Selected case #{:?} | frontier cases: {}",
            selected_idx, frontier_count
        ));
    });
}

fn render_scatter_panel(
    ui: &mut egui::Ui,
    report: &EvalReport,
    scatter_x: &mut usize,
    scatter_y: &mut usize,
    selected_idx: Option<usize>,
) {
    ui.group(|ui| {
        ui.heading("Tradeoff Scatter");
        ui.horizontal(|ui| {
            egui::ComboBox::from_label("X")
                .selected_text(format!("Objective {}", scatter_x))
                .show_ui(ui, |ui| {
                    for i in 0..4 {
                        ui.selectable_value(scatter_x, i, format!("Objective {}", i));
                    }
                });
            egui::ComboBox::from_label("Y")
                .selected_text(format!("Objective {}", scatter_y))
                .show_ui(ui, |ui| {
                    for i in 0..4 {
                        ui.selectable_value(scatter_y, i, format!("Objective {}", i));
                    }
                });
        });

        let desired = egui::vec2(ui.available_width(), 260.0);
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(18));
        painter.rect_stroke(
            rect,
            2.0,
            egui::Stroke::new(1.0, egui::Color32::from_gray(60)),
        );

        let to_screen = |x: f64, y: f64| -> egui::Pos2 {
            let px = rect.left() + (x as f32).clamp(0.0, 1.0) * rect.width();
            let py = rect.bottom() - (y as f32).clamp(0.0, 1.0) * rect.height();
            egui::pos2(px, py)
        };

        for t in [0.0_f32, 0.25, 0.5, 0.75, 1.0] {
            let x = rect.left() + t * rect.width();
            let y = rect.top() + t * rect.height();
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(0.6, egui::Color32::from_gray(35)),
            );
            painter.line_segment(
                [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                egui::Stroke::new(0.6, egui::Color32::from_gray(35)),
            );
        }

        for (idx, case) in report.cases.iter().enumerate() {
            let x = case.normalized[*scatter_x];
            let y = case.normalized[*scatter_y];
            if !x.is_finite() || !y.is_finite() {
                continue;
            }
            let pos = to_screen(x, y);
            let is_front = case.pareto_rank == 1;
            let is_selected = selected_idx == Some(idx);
            let color = if is_selected {
                egui::Color32::YELLOW
            } else if is_front {
                egui::Color32::from_rgb(240, 95, 90)
            } else {
                egui::Color32::from_rgba_unmultiplied(120, 150, 190, 90)
            };
            let radius = if is_selected {
                5.0
            } else if is_front {
                3.8
            } else {
                3.0
            };
            painter.circle_filled(pos, radius, color);
        }
    });
}

fn render_heatmap_panel(ui: &mut egui::Ui, report: &EvalReport) {
    ui.group(|ui| {
        ui.heading("Correlation Heatmap");
        let desired = egui::vec2(ui.available_width(), 300.0);
        let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
        let painter = ui.painter_at(rect);

        let n = 4.0_f32;
        let cell_w = rect.width() / n;
        let cell_h = rect.height() / n;
        for i in 0..4 {
            for j in 0..4 {
                let val = report.correlation_matrix[i][j].clamp(-1.0, 1.0) as f32;
                let color = heat_color(val);
                let cell = egui::Rect::from_min_size(
                    egui::pos2(
                        rect.left() + j as f32 * cell_w,
                        rect.top() + i as f32 * cell_h,
                    ),
                    egui::vec2(cell_w, cell_h),
                );
                painter.rect_filled(cell, 0.0, color);
                painter.rect_stroke(
                    cell,
                    0.0,
                    egui::Stroke::new(1.0, egui::Color32::from_gray(40)),
                );
                painter.text(
                    cell.center(),
                    egui::Align2::CENTER_CENTER,
                    format!("{:+.2}", val),
                    egui::FontId::proportional(12.0),
                    egui::Color32::BLACK,
                );
            }
        }
    });
}

fn heat_color(v: f32) -> egui::Color32 {
    let t = ((v + 1.0) / 2.0).clamp(0.0, 1.0);
    if t <= 0.5 {
        let k = t * 2.0;
        let r = (255.0 * k) as u8;
        let g = (255.0 * k) as u8;
        let b = 255_u8;
        egui::Color32::from_rgb(r, g, b)
    } else {
        let k = (t - 0.5) * 2.0;
        let r = 255_u8;
        let g = (255.0 * (1.0 - k)) as u8;
        let b = (255.0 * (1.0 - k)) as u8;
        egui::Color32::from_rgb(r, g, b)
    }
}

fn render_case_drilldown_panel(
    ui: &mut egui::Ui,
    report: &EvalReport,
    selected_case_index: &mut Option<usize>,
) {
    ui.group(|ui| {
        ui.heading("Case Drilldown");
        ui.label(format!(
            "frontier mean: [{:.4}, {:.4}, {:.4}, {:.4}]",
            report.frontier_mean[0],
            report.frontier_mean[1],
            report.frontier_mean[2],
            report.frontier_mean[3]
        ));
        ui.label(format!(
            "frontier var: [{:.4}, {:.4}, {:.4}, {:.4}]",
            report.frontier_variance[0],
            report.frontier_variance[1],
            report.frontier_variance[2],
            report.frontier_variance[3]
        ));
        ui.separator();
        egui::ScrollArea::vertical()
            .max_height(290.0)
            .show(ui, |ui| {
                for (idx, case) in report.cases.iter().enumerate() {
                    let selected = *selected_case_index == Some(idx);
                    ui.horizontal(|ui| {
                        if ui.selectable_label(selected, &case.case_id).clicked() {
                            *selected_case_index = Some(idx);
                        }
                        ui.weak(format!(
                            "rank={} dom={}",
                            case.pareto_rank, case.domination_count
                        ));
                    });
                    if selected {
                        ui.label(format!(
                            "raw: [{:.4}, {:.4}, {:.4}, {:.4}]",
                            case.raw[0], case.raw[1], case.raw[2], case.raw[3]
                        ));
                        ui.label(format!(
                            "normalized: [{:.4}, {:.4}, {:.4}, {:.4}]",
                            case.normalized[0],
                            case.normalized[1],
                            case.normalized[2],
                            case.normalized[3]
                        ));
                        ui.label(format!(
                            "clamped: [{:.4}, {:.4}, {:.4}, {:.4}]",
                            case.clamped[0], case.clamped[1], case.clamped[2], case.clamped[3]
                        ));
                        ui.separator();
                    }
                }
            });
    });
}
