use eframe::egui;
use crate::state::{AppState, GraphNodeType, UiEvent, UiState};
use crate::session::GuiSession;
use crate::controller::Controller;

pub struct DesignApp {
    session: GuiSession,
    state: AppState,
}

impl DesignApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_custom_fonts(&cc.egui_ctx);

        let store_path = std::path::PathBuf::from(".design_gui_store");
        let session = GuiSession::new("gui_default", store_path)
            .expect("Failed to initialize session");
        
        Self { 
            session,
            state: AppState::default(),
        }
    }

    fn render_graph(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_at_least(egui::vec2(ui.available_width(), 300.0), egui::Sense::click());
        let painter = ui.painter_at(rect);

        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(30));

        let nodes = self.state.graph.nodes.clone();
        let edges = self.state.graph.edges.clone();

        let mut node_positions = std::collections::HashMap::<String, egui::Pos2>::new();

        let l1_nodes: Vec<_> = nodes.iter().filter(|n| n.node_type == GraphNodeType::L1).cloned().collect();
        let l2_nodes: Vec<_> = nodes.iter().filter(|n| n.node_type == GraphNodeType::L2).cloned().collect();
        let ghost_nodes: Vec<_> = nodes.iter().filter(|n| n.node_type == GraphNodeType::Ghost).cloned().collect();

        let x_l1 = rect.left() + 100.0;
        let x_l2 = rect.right() - 100.0;

        for (i, node) in l1_nodes.iter().enumerate() {
            let default_y = rect.top() + (i as f32 + 1.0) * (rect.height() / (l1_nodes.len() as f32 + 1.0));
            let (px, py) = self.state.graph_positions.get(&node.id).copied().unwrap_or((x_l1, default_y));
            let pos = egui::pos2(px, py);
            node_positions.insert(node.id.clone(), pos);

            let is_selected = self.state.selected_node.as_ref() == Some(&node.id);
            let color = egui::Color32::from_rgb(
                (node.score * 255.0) as u8,
                (node.score * 255.0) as u8,
                ((1.0 - node.score) * 255.0) as u8,
            );
            
            if is_selected {
                painter.circle_stroke(pos, 14.0, egui::Stroke::new(2.0, egui::Color32::WHITE));
            }
            
            let hit_rect = egui::Rect::from_center_size(pos, egui::vec2(22.0, 22.0));
            let circle_resp = ui.interact(hit_rect, ui.id().with(&node.id), egui::Sense::click_and_drag());
            if circle_resp.clicked() {
                self.state.selected_node = Some(node.id.clone());
                self.state.selected_detail = Some(format!("L1 Unit: {}\nAmbiguity: {:.2}", node.label, node.score));
                let _ = self.state.ui_state_machine.dispatch(UiEvent::StartEdit);
            }
            if circle_resp.dragged() {
                let p = pos + circle_resp.drag_delta();
                self.state.graph_positions.insert(node.id.clone(), (p.x, p.y));
            }

            painter.circle_filled(pos, 10.0, color);
            painter.text(pos + egui::vec2(0.0, 15.0), egui::Align2::CENTER_TOP, &node.id, egui::FontId::proportional(12.0), egui::Color32::WHITE);
        }

        for (i, node) in l2_nodes.iter().enumerate() {
            let default_y = rect.top() + (i as f32 + 1.0) * (rect.height() / (l2_nodes.len() as f32 + 1.0));
            let (px, py) = self.state.graph_positions.get(&node.id).copied().unwrap_or((x_l2, default_y));
            let pos = egui::pos2(px, py);
            node_positions.insert(node.id.clone(), pos);

            let is_selected = self.state.selected_node.as_ref() == Some(&node.id);
            let color = egui::Color32::from_rgb(
                ((1.0 - node.score) * 255.0) as u8,
                (node.score * 255.0) as u8,
                0,
            );

            let rect_shape = egui::Rect::from_center_size(pos, egui::vec2(20.0, 20.0));
            if is_selected {
                painter.rect_stroke(rect_shape.expand(4.0), 4.0, egui::Stroke::new(2.0, egui::Color32::WHITE));
            }

            let rect_resp = ui.interact(rect_shape, ui.id().with(&node.id), egui::Sense::click_and_drag());
            if rect_resp.clicked() {
                self.state.selected_node = Some(node.id.clone());
                self.state.selected_detail = Some(format!("L2 Concept: {}\nStability: {:.2}", node.id, node.score));
                let _ = self.state.ui_state_machine.dispatch(UiEvent::StartEdit);
            }
            if rect_resp.dragged() {
                let p = pos + rect_resp.drag_delta();
                self.state.graph_positions.insert(node.id.clone(), (p.x, p.y));
            }

            painter.rect_filled(rect_shape, 4.0, color);
            painter.text(pos + egui::vec2(0.0, 15.0), egui::Align2::CENTER_TOP, &node.id, egui::FontId::proportional(12.0), egui::Color32::WHITE);
        }

        for (i, node) in ghost_nodes.iter().enumerate() {
            let default_y = rect.top() + (i as f32 + 1.0) * (rect.height() / (ghost_nodes.len() as f32 + 1.0));
            let default_x = (x_l1 + x_l2) * 0.5;
            let (px, py) = self.state.graph_positions.get(&node.id).copied().unwrap_or((default_x, default_y));
            let pos = egui::pos2(px, py);
            node_positions.insert(node.id.clone(), pos);
            let hit_rect = egui::Rect::from_center_size(pos, egui::vec2(22.0, 22.0));
            let ghost_resp = ui.interact(hit_rect, ui.id().with(&node.id), egui::Sense::click_and_drag());
            if ghost_resp.clicked() {
                self.state.selected_node = Some(node.id.clone());
                self.state.selected_detail = Some(format!("Ghost Draft: {}\nScore: {:.2}", node.id, node.score));
                let _ = self.state.ui_state_machine.dispatch(UiEvent::StartEdit);
            }
            if ghost_resp.dragged() {
                let p = pos + ghost_resp.drag_delta();
                self.state.graph_positions.insert(node.id.clone(), (p.x, p.y));
            }
            let color = egui::Color32::from_rgba_unmultiplied(180, 180, 255, 110);
            painter.circle_filled(pos, 9.0, color);
            painter.circle_stroke(pos, 12.0, egui::Stroke::new(1.2, egui::Color32::LIGHT_BLUE));
            painter.text(
                pos + egui::vec2(0.0, 15.0),
                egui::Align2::CENTER_TOP,
                &node.id,
                egui::FontId::proportional(11.0),
                egui::Color32::LIGHT_BLUE,
            );
        }

        for edge in &edges {
            if let (Some(p1), Some(p2)) = (node_positions.get(&edge.from), node_positions.get(&edge.to)) {
                painter.line_segment([*p1, *p2], egui::Stroke::new(1.5, egui::Color32::from_gray(150)));
            }
        }

        if response.clicked() && !ui.ui_contains_pointer() {
            self.state.selected_node = None;
            self.state.selected_detail = None;
        }
    }

    fn render_advisor(&mut self, ui: &mut egui::Ui) {
        ui.heading("Knowledge Advisor");
        if self.state.missing_info.is_empty() {
            ui.label("設計は現在、十分な具体性を持っています。");
        } else {
            for info in &self.state.missing_info {
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(match info.category {
                            hybrid_vm::InfoCategory::Constraint => "📌 制約",
                            hybrid_vm::InfoCategory::Boundary => "🔍 境界",
                            hybrid_vm::InfoCategory::Metric => "📊 指標",
                            hybrid_vm::InfoCategory::Objective => "🎯 目標",
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("回答する").clicked() {
                                // プロンプトを一部入力欄にコピー
                                self.state.input_text = format!("{} について: ", info.prompt);
                            }
                        });
                    });
                    ui.label(&info.prompt);
                });
            }
        }

        ui.separator();
        ui.heading("Graph Edit");
        ui.horizontal(|ui| {
            let selected = self.state.selected_node.clone().unwrap_or_else(|| "-".to_string());
            ui.label(format!("Selected: {selected}"));
        });
        ui.horizontal(|ui| {
            if ui.button("Start Edge").clicked() {
                Controller::begin_edge(&mut self.state);
            }
            if ui.button("Connect To Selected").clicked() {
                Controller::connect_to_selected(&mut self.state);
            }
            if ui.button("Delete Selected").clicked() {
                Controller::remove_selected_node(&mut self.state);
            }
        });
        if let Some(from) = &self.state.edge_builder_from {
            ui.label(format!("Edge builder source: {from}"));
        }

        ui.separator();
        ui.heading("Generated Drafts");
        if self.state.drafts.is_empty() {
            ui.label("提案ドラフトはありません。");
        } else {
            for draft in self.state.drafts.clone() {
                ui.group(|ui| {
                    ui.label(format!(
                        "{} (impact: {:+.2}%)",
                        draft.draft_id,
                        draft.stability_impact * 100.0
                    ));
                    ui.label(&draft.prompt);
                    if ui.button("Adopt").clicked() {
                        Controller::adopt_draft(&mut self.session, &mut self.state, &draft.draft_id);
                    }
                });
            }
        }
    }
}

impl eframe::App for DesignApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("DesignBrainModel G3-C");
                ui.separator();
                ui.label(format!("Session: {}", self.session.id));
                ui.separator();
                ui.label(format!("State: {:?}", self.state.ui_state_machine.current_state()));
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Clear Session").clicked() {
                        Controller::clear_session(&mut self.session, &mut self.state);
                    }
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
                ui.label("Add Requirement:");
                let text_resp = ui.add(egui::TextEdit::singleline(&mut self.state.input_text).hint_text("追加の要件や回答を入力..."));
                if text_resp.changed() {
                    let _ = self.state.ui_state_machine.dispatch(UiEvent::StartEdit);
                }
                
                if ui.button("Analyze (Append)").clicked() {
                    Controller::analyze_append(&mut self.session, &mut self.state);
                }
            });

            if let Some(err) = &self.state.last_error {
                ui.colored_label(egui::Color32::RED, err);
            }
        });

        egui::SidePanel::right("advisor_panel").min_width(300.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.render_advisor(ui);
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                match self.state.ui_state_machine.current_state() {
                    UiState::Analyzing => {
                        ui.centered_and_justified(|ui| {
                            ui.spinner();
                            ui.label("Knowledge Recall & Analysis...");
                        });
                    }
                    _ => {
                        ui.heading("Causal Design Graph");
                        self.render_graph(ui);
                        
                        if let Some(detail) = &self.state.selected_detail {
                            ui.group(|ui| {
                                ui.label(egui::RichText::new("Selected Node Detail").strong());
                                ui.label(detail);
                            });
                        }
                        
                        ui.separator();

                        ui.heading("Specification Cards");
                        let cards = self.state.cards.clone();
                        for card in cards {
                            let card_key = format!("L2-{}", card.id.0);
                            let has_grounding = !card.grounding_data.is_empty();
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(&card_key).strong());
                                    ui.label(format!("Parent: L1-{}", card.parent_id.0));
                                    if has_grounding {
                                        ui.colored_label(egui::Color32::LIGHT_GREEN, "Grounded");
                                    } else {
                                        ui.colored_label(egui::Color32::YELLOW, "Needs Grounding");
                                    }
                                });
                                if !card.metrics.is_empty() {
                                    ui.label(format!("Metrics: {}", card.metrics.join(", ")));
                                }
                                if !card.methods.is_empty() {
                                    ui.label(format!("Methods: {}", card.methods.join(", ")));
                                }

                                let current_text = self
                                    .state
                                    .card_edit_buffers
                                    .get(&card_key)
                                    .cloned()
                                    .unwrap_or_default();
                                let mut edited_text = current_text.clone();
                                ui.add(
                                    egui::TextEdit::singleline(&mut edited_text)
                                        .hint_text("Refine detail (e.g. p99 latency < 120ms)"),
                                );
                                if edited_text != current_text {
                                    self.state
                                        .card_edit_buffers
                                        .insert(card_key.clone(), edited_text.clone());
                                }
                                ui.horizontal(|ui| {
                                    if ui.button("Save Detail").clicked() && !edited_text.trim().is_empty() {
                                        let text = edited_text.clone();
                                        Controller::refine_card(&mut self.session, &mut self.state, &card_key, &text);
                                    }
                                    if ui.button("Grounding").clicked() {
                                        Controller::ground_card(&mut self.session, &mut self.state, &card_key);
                                    }
                                });
                            });
                        }

                        if let Some(exp) = &self.state.explanation {
                            ui.separator();
                            ui.heading("Overview");
                            ui.group(|ui| {
                                ui.label(&exp.summary);
                                ui.weak(&exp.detail);
                            });
                        }
                    }
                }
            });
        });
    }
}

fn setup_custom_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let font_paths = [
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/Library/Fonts/Arial Unicode.ttf",
    ];
    let mut font_loaded = false;
    for path in font_paths {
        if let Ok(font_data) = std::fs::read(path) {
            fonts.font_data.insert("japanese_font".to_owned(), egui::FontData::from_owned(font_data));
            font_loaded = true;
            break;
        }
    }
    if font_loaded {
        fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "japanese_font".to_owned());
        fonts.families.entry(egui::FontFamily::Monospace).or_default().push("japanese_font".to_owned());
        ctx.set_fonts(fonts);
    }
}
