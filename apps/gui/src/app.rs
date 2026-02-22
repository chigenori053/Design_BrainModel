use eframe::egui;
use crate::state::AppState;
use crate::state::{UiEvent, UiState};
use crate::session::GuiSession;
use crate::controller::Controller;

pub struct DesignApp {
    session: GuiSession,
    state: AppState,
}

impl DesignApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let store_path = std::path::PathBuf::from(".design_gui_store");
        let session = GuiSession::new("gui_default", store_path)
            .expect("Failed to initialize session");
        
        Self { 
            session,
            state: AppState::default(),
        }
    }
}

impl eframe::App for DesignApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("DesignBrainModel GUI v1.1");
                ui.separator();
                ui.label(format!("Session: {} (Created: {})", self.session.id, self.session.created_at));
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Redo").clicked() {
                        Controller::redo(&mut self.session, &mut self.state);
                    }
                    if ui.button("Undo").clicked() {
                        Controller::undo(&mut self.session, &mut self.state);
                    }
                });
            });
            
            ui.add_space(8.0);
            
            ui.horizontal(|ui| {
                ui.label("Input Text:");
                let response = ui.text_edit_singleline(&mut self.state.input_text);
                if response.changed() {
                    let current = self.state.ui_state_machine.current_state();
                    if matches!(current, UiState::Idle) {
                        let _ = self.state.ui_state_machine.dispatch(UiEvent::StartEdit);
                    } else if matches!(current, UiState::Reviewing | UiState::Error) {
                        let _ = self.state.ui_state_machine.dispatch(UiEvent::Revise);
                    }
                }
                
                if ui.button("Analyze").clicked() {
                    Controller::analyze(&mut self.session, &mut self.state);
                }
                if ui.button("Reset").clicked() {
                    let _ = self.state.ui_state_machine.dispatch(UiEvent::Reset);
                    self.state.last_error = None;
                }
            });

            if let Some(err) = &self.state.last_error {
                ui.colored_label(egui::Color32::RED, err);
            }
            ui.label(format!("UI State: {:?}", self.state.ui_state_machine.current_state()));
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_min_width(350.0);
                        ui.heading("L1 Units");
                        for l1 in &self.state.l1_units {
                            ui.group(|ui| {
                                ui.label(format!("ID: {}", l1.id.0));
                                if let Some(obj) = &l1.objective {
                                    ui.label(format!("Objective: {}", obj));
                                }
                                ui.label(format!("Constraints: {:?}", l1.constraints));
                                ui.label(format!("Ambiguity: {:.2}", l1.ambiguity_score));
                            });
                        }
                    });

                    ui.separator();

                    ui.vertical(|ui| {
                        ui.set_min_width(350.0);
                        ui.heading("L2 Concepts");
                        for l2 in &self.state.l2_units {
                            ui.group(|ui| {
                                ui.label(format!("Concept: {}", l2.id.0));
                                ui.label(format!("Stability: {:.2}", l2.stability_score));
                                for req in &l2.derived_requirements {
                                    ui.label(format!("- {:?}: {:.2}", req.kind, req.strength));
                                }
                            });
                        }
                    });
                });

                if let Some(exp) = &self.state.explanation {
                    ui.separator();
                    ui.heading("Explanation");
                    ui.group(|ui| {
                        ui.label(&exp.summary);
                        ui.add_space(4.0);
                        ui.weak(&exp.detail);
                    });
                }

                if let Some(snap) = &self.state.snapshot {
                    ui.separator();
                    ui.heading("Snapshot");
                    ui.label(format!("L1 Hash: {}", snap.l1_hash));
                    ui.label(format!("L2 Hash: {}", snap.l2_hash));
                    ui.label(format!("Version: {}", snap.version));
                }
            });
        });
    }
}
