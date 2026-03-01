use std::sync::{Arc, RwLock};
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use agent_core::domain::hash::compute_hash;
use agent_core::domain::{AppState, ParetoResult, ProposedDiff, UnifiedDesignState};
use eframe::egui;
use crate::persistence::{
    app_state_from_persisted, load_checkpoint, load_checkpoint_at_version, load_checkpoint_entries,
    save_checkpoint, CheckpointEntry,
};

pub type SharedAppState = Arc<RwLock<AppState>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Overview,
    Editor,
}

impl Default for Tab {
    fn default() -> Self {
        Self::Overview
    }
}

#[derive(Debug, Default)]
pub struct GuiViewState {
    pub selected_node: Option<String>,
    pub editor_buffer: String,
    pub active_tab: Tab,
    pub scroll_position: f32,
    pub error_message: Option<String>,
    pub pareto_result: Option<String>,
    pub latest_pareto: Option<ParetoResult>,
    pub suggested_diffs: Vec<ProposedDiff>,
    pub analyze_metrics: Option<AnalyzeMetrics>,
    pub show_history_modal: bool,
    pub checkpoint_entries: Vec<CheckpointEntry>,
    pub suggest_metrics: SuggestMetrics,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyzeMetrics {
    pub evaluate_duration_ms: u128,
    pub pareto_duration_ms: u128,
    pub suggest_duration_ms: Option<u128>,
    pub timestamp: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SuggestMetrics {
    pub suggestion_count: u64,
    pub accepted_count: u64,
    pub rejected_by_guard: u64,
    pub avg_consistency_gain: f64,
    pub avg_structural_gain: f64,
    pub avg_dependency_gain: f64,
    applied_count: u64,
    sum_consistency_gain: f64,
    sum_structural_gain: f64,
    sum_dependency_gain: f64,
}

impl Default for SuggestMetrics {
    fn default() -> Self {
        Self {
            suggestion_count: 0,
            accepted_count: 0,
            rejected_by_guard: 0,
            avg_consistency_gain: 0.0,
            avg_structural_gain: 0.0,
            avg_dependency_gain: 0.0,
            applied_count: 0,
            sum_consistency_gain: 0.0,
            sum_structural_gain: 0.0,
            sum_dependency_gain: 0.0,
        }
    }
}

pub struct DesignApp {
    pub domain_state: SharedAppState,
    pub view_state: GuiViewState,
    pub checkpoint_path: PathBuf,
}

#[derive(Debug, Clone)]
pub enum GuiEvent {
    ApplyDiff(ProposedDiff),
    Analyze,
    Undo,
    Redo,
    Save,
}

pub fn handle_event(event: GuiEvent, state: &SharedAppState) -> Result<(), String> {
    let mut s = state
        .write()
        .map_err(|_| "domain state write lock poisoned".to_string())?;

    match event {
        GuiEvent::ApplyDiff(diff) => {
            s.begin_tx().map_err(|e| format!("begin_tx failed: {e:?}"))?;

            if let Err(err) = s.apply_diff(diff) {
                let _ = s.abort_tx();
                return Err(format!("apply_diff failed: {err:?}"));
            }

            if let Err(err) = s.commit_tx() {
                let _ = s.abort_tx();
                return Err(format!("commit_tx failed: {err:?}"));
            }
        }
        GuiEvent::Analyze => s
            .evaluate_now()
            .map_err(|e| format!("analyze failed: {e:?}"))?,
        GuiEvent::Undo => s.undo().map_err(|e| format!("undo failed: {e:?}"))?,
        GuiEvent::Redo => s.redo().map_err(|e| format!("redo failed: {e:?}"))?,
        GuiEvent::Save => {
            // Persistent save is intentionally out of scope in Sprint 2.
        }
    }

    Ok(())
}

impl DesignApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self::new_with_checkpoint_path(PathBuf::from("project.dbm"))
    }

    pub fn new_headless() -> Self {
        Self::new_with_checkpoint_path(PathBuf::from("project.dbm"))
    }

    pub fn new_with_checkpoint_path(checkpoint_path: PathBuf) -> Self {
        let loaded_state = match load_checkpoint(&checkpoint_path) {
            Ok(Some(persisted)) => app_state_from_persisted(persisted),
            Ok(None) => AppState::default(),
            Err(_) => AppState::default(),
        };
        let domain_state = Arc::new(RwLock::new(loaded_state));
        let editor_buffer = domain_state
            .read()
            .ok()
            .map(|s| serialize_uds(&s.uds))
            .unwrap_or_default();

        Self {
            domain_state,
            view_state: GuiViewState {
                editor_buffer,
                ..GuiViewState::default()
            },
            checkpoint_path,
        }
    }

    pub fn sync_editor_to_domain(&mut self) {
        let parsed = match parse_editor_buffer(&self.view_state.editor_buffer) {
            Ok(v) => v,
            Err(err) => {
                self.view_state.error_message = Some(err);
                return;
            }
        };

        let mut s = match self.domain_state.write() {
            Ok(guard) => guard,
            Err(_) => {
                self.view_state.error_message = Some("domain state write lock poisoned".to_string());
                return;
            }
        };

        if let Err(err) = s.begin_tx() {
            self.view_state.error_message = Some(format!("begin_tx failed: {err:?}"));
            return;
        }

        if let Err(err) = s.replace_uds(parsed) {
            let _ = s.abort_tx();
            self.view_state.error_message = Some(format!("replace_uds failed: {err:?}"));
            return;
        }

        if let Err(err) = s.commit_tx() {
            let _ = s.abort_tx();
            self.view_state.error_message = Some(format!("commit_tx failed: {err:?}"));
            return;
        }

        self.view_state.error_message = None;
    }

    fn refresh_editor_from_domain(&mut self) {
        if let Ok(s) = self.domain_state.read() {
            self.view_state.editor_buffer = serialize_uds(&s.uds);
        }
    }

    fn apply_diff_card(&mut self, diff: ProposedDiff) {
        match handle_event(GuiEvent::ApplyDiff(diff), &self.domain_state) {
            Ok(()) => {
                self.refresh_editor_from_domain();
                self.view_state.error_message = None;
            }
            Err(err) => {
                self.view_state.error_message = Some(err);
            }
        }
    }

    pub fn trigger_analyze(&mut self) {
        let start_eval = Instant::now();
        match handle_event(GuiEvent::Analyze, &self.domain_state) {
            Ok(()) => {
                let eval_duration_ms = start_eval.elapsed().as_millis();
                let start_pareto = Instant::now();
                match self.compute_pareto_for_view() {
                Ok(result) => {
                    self.view_state.pareto_result = Some(format_pareto_result(&result));
                    self.view_state.latest_pareto = Some(result);
                    self.view_state.suggested_diffs.clear();
                    let pareto_duration_ms = start_pareto.elapsed().as_millis();
                    let mut timestamp = now_timestamp_ms();
                    if let Some(prev) = self.view_state.analyze_metrics.as_ref()
                        && timestamp <= prev.timestamp
                    {
                        timestamp = prev.timestamp.saturating_add(1);
                    }
                    self.view_state.analyze_metrics = Some(AnalyzeMetrics {
                        evaluate_duration_ms: eval_duration_ms,
                        pareto_duration_ms,
                        suggest_duration_ms: None,
                        timestamp,
                    });
                    self.view_state.error_message = None;
                }
                Err(err) => self.view_state.error_message = Some(err),
            }
            }
            Err(err) => self.view_state.error_message = Some(err),
        }
    }

    pub fn trigger_suggest(&mut self) {
        let Some(metrics) = self.view_state.analyze_metrics.clone() else {
            self.view_state.error_message = Some("Suggest requires Analyze metrics".to_string());
            return;
        };

        if metrics.evaluate_duration_ms >= 50 || metrics.pareto_duration_ms >= 100 {
            self.view_state.error_message = Some(
                "Suggest disabled: analyze timing threshold exceeded".to_string(),
            );
            self.view_state.suggested_diffs.clear();
            return;
        }

        let Some(pareto) = self.view_state.latest_pareto.clone() else {
            self.view_state.error_message =
                Some("Suggest requires Analyze to run first".to_string());
            return;
        };

        let start_suggest = Instant::now();
        let (candidate_count, suggestions) = match self.domain_state.read() {
            Ok(s) => {
                let candidate_count = estimate_candidate_count(&s);
                match s.suggest_diffs_from_analysis(&pareto) {
                    Ok(v) => (candidate_count, v),
                Err(err) => {
                    self.view_state.error_message = Some(format!("suggest failed: {err:?}"));
                    return;
                }
                }
            }
            Err(_) => {
                self.view_state.error_message = Some("domain state read lock poisoned".to_string());
                return;
            }
        };
        let suggest_duration_ms = start_suggest.elapsed().as_millis();

        if suggest_duration_ms >= 50 {
            self.view_state.error_message =
                Some("Suggest disabled: suggest timing threshold exceeded".to_string());
            self.view_state.suggested_diffs.clear();
        } else {
            self.view_state.error_message = None;
            self.view_state.suggest_metrics.suggestion_count += candidate_count as u64;
            self.view_state.suggest_metrics.accepted_count += suggestions.len() as u64;
            self.view_state.suggest_metrics.rejected_by_guard +=
                candidate_count.saturating_sub(suggestions.len()) as u64;
            self.view_state.suggested_diffs = suggestions;
        }

        let mut timestamp = now_timestamp_ms();
        if timestamp <= metrics.timestamp {
            timestamp = metrics.timestamp.saturating_add(1);
        }
        self.view_state.analyze_metrics = Some(AnalyzeMetrics {
            evaluate_duration_ms: metrics.evaluate_duration_ms,
            pareto_duration_ms: metrics.pareto_duration_ms,
            suggest_duration_ms: Some(suggest_duration_ms),
            timestamp,
        });
    }

    pub fn apply_suggested_diff_at(&mut self, index: usize) {
        let Some(diff) = self.view_state.suggested_diffs.get(index).cloned() else {
            return;
        };
        let before_eval = self
            .domain_state
            .read()
            .ok()
            .map(|s| s.evaluation.clone());
        self.apply_diff_card(diff);
        let after_eval = self
            .domain_state
            .read()
            .ok()
            .map(|s| s.evaluation.clone());
        if let (Some(before), Some(after)) = (before_eval, after_eval) {
            self.record_suggest_apply_delta(&before, &after);
        }
    }

    fn record_suggest_apply_delta(&mut self, before: &agent_core::domain::DesignScoreVector, after: &agent_core::domain::DesignScoreVector) {
        let delta_consistency = (after.consistency as f64 - before.consistency as f64) / 100.0;
        let delta_structural =
            (after.structural_integrity as f64 - before.structural_integrity as f64) / 100.0;
        let delta_dependency =
            (after.dependency_soundness as f64 - before.dependency_soundness as f64) / 100.0;

        let metrics = &mut self.view_state.suggest_metrics;
        metrics.applied_count += 1;
        metrics.sum_consistency_gain += delta_consistency;
        metrics.sum_structural_gain += delta_structural;
        metrics.sum_dependency_gain += delta_dependency;

        let n = metrics.applied_count as f64;
        metrics.avg_consistency_gain = metrics.sum_consistency_gain / n;
        metrics.avg_structural_gain = metrics.sum_structural_gain / n;
        metrics.avg_dependency_gain = metrics.sum_dependency_gain / n;
    }
}

impl eframe::App for DesignApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(self.view_state.active_tab == Tab::Overview, "Overview")
                    .clicked()
                {
                    self.view_state.active_tab = Tab::Overview;
                }
                if ui
                    .selectable_label(self.view_state.active_tab == Tab::Editor, "Editor")
                    .clicked()
                {
                    self.view_state.active_tab = Tab::Editor;
                }

                ui.separator();

                if ui.button("Analyze").clicked() {
                    self.trigger_analyze();
                }
                if ui.button("Suggest").clicked() {
                    self.trigger_suggest();
                }

                if ui.button("Undo").clicked() {
                    match handle_event(GuiEvent::Undo, &self.domain_state) {
                        Ok(()) => {
                            self.refresh_editor_from_domain();
                            self.view_state.error_message = None;
                        }
                        Err(err) => self.view_state.error_message = Some(err),
                    }
                }

                if ui.button("Redo").clicked() {
                    match handle_event(GuiEvent::Redo, &self.domain_state) {
                        Ok(()) => {
                            self.refresh_editor_from_domain();
                            self.view_state.error_message = None;
                        }
                        Err(err) => self.view_state.error_message = Some(err),
                    }
                }

                if ui.button("Save").clicked() {
                    let snapshot = {
                        let guard = match self.domain_state.read() {
                            Ok(v) => v,
                            Err(_) => {
                                self.view_state.error_message =
                                    Some("domain state read lock poisoned".to_string());
                                return;
                            }
                        };
                        guard.clone()
                    };

                    match save_checkpoint(&snapshot, &self.checkpoint_path) {
                        Ok(()) => {
                            self.view_state.error_message = None;
                            self.refresh_history_entries();
                        }
                        Err(err) => self.view_state.error_message = Some(err.to_string()),
                    }
                }

                if ui.button("History").clicked() {
                    self.refresh_history_entries();
                    self.view_state.show_history_modal = true;
                }
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.columns(2, |columns| {
                self.render_main_column(&mut columns[0]);
                self.render_pareto_column(&mut columns[1]);
            });
        });

        self.render_history_modal(ctx);

        self.view_state.scroll_position += ctx.input(|i| i.raw_scroll_delta.y);
    }
}

impl DesignApp {
    fn compute_pareto_for_view(&self) -> Result<ParetoResult, String> {
        let s = self
            .domain_state
            .read()
            .map_err(|_| "domain state read lock poisoned".to_string())?;
        s.analyze_pareto()
            .map_err(|e| format!("pareto analyze failed: {e:?}"))
    }

    fn refresh_history_entries(&mut self) {
        match load_checkpoint_entries(&self.checkpoint_path) {
            Ok(mut entries) => {
                entries.sort_by(|a, b| b.version_id.cmp(&a.version_id));
                self.view_state.checkpoint_entries = entries;
            }
            Err(err) => self.view_state.error_message = Some(err.to_string()),
        }
    }

    fn restore_checkpoint_version(&mut self, version_id: u64) {
        match load_checkpoint_at_version(&self.checkpoint_path, version_id) {
            Ok(Some(restored)) => {
                if let Ok(mut s) = self.domain_state.write() {
                    *s = restored;
                }
                self.refresh_editor_from_domain();
                self.view_state.error_message = None;
            }
            Ok(None) => {
                self.view_state.error_message = Some("selected checkpoint not found".to_string())
            }
            Err(err) => self.view_state.error_message = Some(err.to_string()),
        }
    }

    fn render_history_modal(&mut self, ctx: &egui::Context) {
        if !self.view_state.show_history_modal {
            return;
        }

        let mut open = self.view_state.show_history_modal;
        egui::Window::new("Checkpoint History")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                if self.view_state.checkpoint_entries.is_empty() {
                    ui.weak("No checkpoint entries yet.");
                    return;
                }
                let entries = self.view_state.checkpoint_entries.clone();
                for entry in entries {
                    ui.group(|ui| {
                        let kind = if entry.is_base { "Base" } else { "delta" };
                        ui.label(format!(
                            "#{} ManualSave ({kind}) ts={}",
                            entry.version_id, entry.timestamp
                        ));
                        if ui.button("Restore").clicked() {
                            self.restore_checkpoint_version(entry.version_id);
                            self.view_state.show_history_modal = false;
                        }
                    });
                }
            });
        self.view_state.show_history_modal = open;
    }

    fn render_main_column(&mut self, ui: &mut egui::Ui) {
        self.render_evaluation_summary(ui);
        ui.separator();

        self.render_diff_card(ui);
        ui.separator();

        if self.view_state.active_tab == Tab::Editor {
            self.render_editor(ui);
        } else {
            self.render_overview(ui);
        }

        if let Some(err) = &self.view_state.error_message {
            ui.separator();
            ui.colored_label(egui::Color32::RED, err);
        }
    }

    fn render_pareto_column(&mut self, ui: &mut egui::Ui) {
        ui.heading("Pareto Analyze");
        if let Some(summary) = &self.view_state.pareto_result {
            ui.monospace(summary);
        } else {
            ui.weak("Press Analyze to compute non-destructive Pareto front.");
        }

        if let Some(metrics) = &self.view_state.analyze_metrics {
            ui.separator();
            ui.label(format!("Eval: {} ms", metrics.evaluate_duration_ms));
            ui.label(format!("Pareto: {} ms", metrics.pareto_duration_ms));
            if let Some(suggest_ms) = metrics.suggest_duration_ms {
                ui.label(format!("Suggest: {} ms", suggest_ms));
            }
        }

        if !self.view_state.suggested_diffs.is_empty() {
            ui.separator();
            ui.heading("Suggested Diffs");
            let cards = self.view_state.suggested_diffs.clone();
            for (index, diff) in cards.iter().enumerate() {
                ui.group(|ui| {
                    ui.label(format!("Diff {index}: {:?}", diff));
                    if ui.button("Apply").clicked() {
                        self.apply_suggested_diff_at(index);
                    }
                });
            }
        }

        ui.separator();
        ui.heading("Suggest Metrics");
        ui.label(format!(
            "Accepted: {} / {}",
            self.view_state.suggest_metrics.accepted_count,
            self.view_state.suggest_metrics.suggestion_count
        ));
        ui.label(format!(
            "Rejected by Guard: {}",
            self.view_state.suggest_metrics.rejected_by_guard
        ));
        ui.label(format!(
            "Avg Δconsistency: {:+.3}",
            self.view_state.suggest_metrics.avg_consistency_gain
        ));
        ui.label(format!(
            "Avg Δpropagation_quality: {:+.3}",
            self.view_state.suggest_metrics.avg_structural_gain
        ));
        ui.label(format!(
            "Avg Δcycle_quality: {:+.3}",
            self.view_state.suggest_metrics.avg_dependency_gain
        ));
    }

    fn render_evaluation_summary(&self, ui: &mut egui::Ui) {
        ui.heading("EvaluationSummary");

        if let Ok(s) = self.domain_state.read() {
            ui.horizontal(|ui| {
                ui.label(format!("consistency: {}", s.evaluation.consistency));
                ui.separator();
                ui.label(format!(
                    "propagation_quality: {}",
                    s.evaluation.structural_integrity
                ));
                ui.separator();
                ui.label(format!(
                    "cycle_quality: {}",
                    s.evaluation.dependency_soundness
                ));
                ui.separator();
                ui.label(format!("uds_hash: {}", compute_hash(&s.uds)));
            });
        }
    }

    fn render_diff_card(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.heading("DiffCard: Quick Add Node");
            if ui.button("Apply").clicked() {
                let key = self
                    .view_state
                    .selected_node
                    .clone()
                    .unwrap_or_else(|| "quick-node".to_string());
                let value = if self.view_state.editor_buffer.trim().is_empty() {
                    "quick value".to_string()
                } else {
                    self.view_state.editor_buffer.trim().to_string()
                };

                self.apply_diff_card(ProposedDiff::UpsertNode { key, value });
            }
        });
    }

    fn render_overview(&mut self, ui: &mut egui::Ui) {
        ui.heading("UDS Nodes");

        if let Ok(s) = self.domain_state.read() {
            if s.uds.nodes.is_empty() {
                ui.weak("No nodes");
                return;
            }

            egui::ScrollArea::vertical().show(ui, |ui| {
                for (key, value) in &s.uds.nodes {
                    let selected = self.view_state.selected_node.as_deref() == Some(key.as_str());
                    if ui
                        .selectable_label(selected, format!("{} = {}", key, value))
                        .clicked()
                    {
                        self.view_state.selected_node = Some(key.clone());
                    }
                }
            });
        }
    }

    fn render_editor(&mut self, ui: &mut egui::Ui) {
        ui.heading("BlockTextEditor -> UDS");
        ui.label("format: node:<key>=<value> / dep:<key>->a,b");

        let response = ui.add(
            egui::TextEdit::multiline(&mut self.view_state.editor_buffer)
                .desired_rows(20)
                .lock_focus(true),
        );

        if response.lost_focus() {
            self.sync_editor_to_domain();
        }
    }
}

fn format_pareto_result(result: &ParetoResult) -> String {
    let mut lines = Vec::new();
    lines.push(format!("frontier_indices: {:?}", result.frontier_indices));
    for (index, score) in result.scores.iter().enumerate() {
        lines.push(format!(
            "#{index} => [consistency={}, propagation_q={}, cycle_q={}]",
            score.consistency, score.structural_integrity, score.dependency_soundness
        ));
    }
    lines.join("\n")
}

fn now_timestamp_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn estimate_candidate_count(state: &AppState) -> usize {
    let mut count = 0;
    let eval = state.evaluation.clone();

    if eval.consistency < 80 {
        let empty_nodes = state
            .uds
            .nodes
            .values()
            .filter(|value| value.trim().is_empty())
            .count();
        count += empty_nodes;
        if state.uds.nodes.len() > 1 {
            count += empty_nodes;
        }
    }

    if eval.structural_integrity < 75 {
        count += state
            .uds
            .dependencies
            .keys()
            .filter(|key| !state.uds.nodes.contains_key(*key))
            .count();
    }

    if eval.dependency_soundness < 85 {
        count += state
            .uds
            .dependencies
            .iter()
            .filter(|(key, deps)| {
                let filtered = deps
                    .iter()
                    .filter(|dep| *dep != *key && state.uds.nodes.contains_key(*dep))
                    .cloned()
                    .collect::<Vec<_>>();
                &filtered != *deps
            })
            .count();
    }

    count
}

fn parse_editor_buffer(input: &str) -> Result<UnifiedDesignState, String> {
    let mut uds = UnifiedDesignState::default();

    for (index, raw_line) in input.lines().enumerate() {
        let line_no = index + 1;
        let line = raw_line.trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(body) = line.strip_prefix("node:") {
            let (key, value) = body
                .split_once('=')
                .ok_or_else(|| format!("line {line_no}: node format must be node:key=value"))?;
            let key = key.trim();
            if key.is_empty() {
                return Err(format!("line {line_no}: empty node key"));
            }
            uds.nodes.insert(key.to_string(), value.trim().to_string());
            continue;
        }

        if let Some(body) = line.strip_prefix("dep:") {
            let (key, deps_raw) = body
                .split_once("->")
                .ok_or_else(|| format!("line {line_no}: dep format must be dep:key->a,b"))?;
            let key = key.trim();
            if key.is_empty() {
                return Err(format!("line {line_no}: empty dependency owner key"));
            }

            let deps = deps_raw
                .split(',')
                .map(str::trim)
                .filter(|entry| !entry.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>();

            uds.dependencies.insert(key.to_string(), deps);
            continue;
        }

        return Err(format!(
            "line {line_no}: unknown prefix, use node: or dep:"
        ));
    }

    Ok(uds)
}

fn serialize_uds(uds: &UnifiedDesignState) -> String {
    let mut lines = Vec::new();

    for (key, value) in &uds.nodes {
        lines.push(format!("node:{key}={value}"));
    }

    for (key, deps) in &uds.dependencies {
        lines.push(format!("dep:{key}->{}", deps.join(",")));
    }

    lines.join("\n")
}
