use crate::controller::Controller;
use crate::detail_view::{debug_mode_enabled, render_explanation};
use crate::session::GuiSession;
use crate::state::{AppState, GraphNodeType, UiEvent, UiState};
use eframe::egui;

pub struct DesignApp {
    session: GuiSession,
    state: AppState,
}

impl DesignApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        setup_custom_fonts(&cc.egui_ctx);

        let store_path = std::path::PathBuf::from(".design_gui_store");
        let session =
            GuiSession::new("gui_default", store_path).expect("Failed to initialize session");

        Self {
            session,
            state: AppState::default(),
        }
    }

    #[allow(dead_code)]
    fn render_graph(&mut self, ui: &mut egui::Ui) {
        let (rect, response) = ui.allocate_at_least(
            egui::vec2(ui.available_width(), 300.0),
            egui::Sense::click(),
        );
        let painter = ui.painter_at(rect);

        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(30));

        let nodes = self.state.graph.nodes.clone();
        let edges = self.state.graph.edges.clone();

        let mut node_positions = std::collections::HashMap::<String, egui::Pos2>::new();

        let l1_nodes: Vec<_> = nodes
            .iter()
            .filter(|n| n.node_type == GraphNodeType::L1)
            .cloned()
            .collect();
        let l2_nodes: Vec<_> = nodes
            .iter()
            .filter(|n| n.node_type == GraphNodeType::L2)
            .cloned()
            .collect();
        let ghost_nodes: Vec<_> = nodes
            .iter()
            .filter(|n| n.node_type == GraphNodeType::Ghost)
            .cloned()
            .collect();

        let x_l1 = rect.left() + 100.0;
        let x_l2 = rect.right() - 100.0;

        for (i, node) in l1_nodes.iter().enumerate() {
            let default_y =
                rect.top() + (i as f32 + 1.0) * (rect.height() / (l1_nodes.len() as f32 + 1.0));
            let (px, py) = self
                .state
                .graph_positions
                .get(&node.id)
                .copied()
                .unwrap_or((x_l1, default_y));
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
            let circle_resp = ui.interact(
                hit_rect,
                ui.id().with(&node.id),
                egui::Sense::click_and_drag(),
            );
            if circle_resp.clicked() {
                self.state.selected_node = Some(node.id.clone());
                self.state.selected_detail = Some(format!(
                    "L1 Unit: {}\nAmbiguity: {:.2}",
                    node.label, node.score
                ));
                let _ = self.state.ui_state_machine.dispatch(UiEvent::StartEdit);
            }
            if circle_resp.dragged() {
                let p = pos + circle_resp.drag_delta();
                self.state
                    .graph_positions
                    .insert(node.id.clone(), (p.x, p.y));
            }

            painter.circle_filled(pos, 10.0, color);
            painter.text(
                pos + egui::vec2(0.0, 15.0),
                egui::Align2::CENTER_TOP,
                &node.id,
                egui::FontId::proportional(12.0),
                egui::Color32::WHITE,
            );
        }

        for (i, node) in l2_nodes.iter().enumerate() {
            let default_y =
                rect.top() + (i as f32 + 1.0) * (rect.height() / (l2_nodes.len() as f32 + 1.0));
            let (px, py) = self
                .state
                .graph_positions
                .get(&node.id)
                .copied()
                .unwrap_or((x_l2, default_y));
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
                painter.rect_stroke(
                    rect_shape.expand(4.0),
                    4.0,
                    egui::Stroke::new(2.0, egui::Color32::WHITE),
                );
            }

            let rect_resp = ui.interact(
                rect_shape,
                ui.id().with(&node.id),
                egui::Sense::click_and_drag(),
            );
            if rect_resp.clicked() {
                self.state.selected_node = Some(node.id.clone());
                self.state.selected_detail = Some(format!(
                    "L2 Concept: {}\nStability: {:.2}",
                    node.id, node.score
                ));
                let _ = self.state.ui_state_machine.dispatch(UiEvent::StartEdit);
            }
            if rect_resp.dragged() {
                let p = pos + rect_resp.drag_delta();
                self.state
                    .graph_positions
                    .insert(node.id.clone(), (p.x, p.y));
            }

            painter.rect_filled(rect_shape, 4.0, color);
            painter.text(
                pos + egui::vec2(0.0, 15.0),
                egui::Align2::CENTER_TOP,
                &node.id,
                egui::FontId::proportional(12.0),
                egui::Color32::WHITE,
            );
        }

        for (i, node) in ghost_nodes.iter().enumerate() {
            let default_y =
                rect.top() + (i as f32 + 1.0) * (rect.height() / (ghost_nodes.len() as f32 + 1.0));
            let default_x = (x_l1 + x_l2) * 0.5;
            let (px, py) = self
                .state
                .graph_positions
                .get(&node.id)
                .copied()
                .unwrap_or((default_x, default_y));
            let pos = egui::pos2(px, py);
            node_positions.insert(node.id.clone(), pos);
            let hit_rect = egui::Rect::from_center_size(pos, egui::vec2(22.0, 22.0));
            let ghost_resp = ui.interact(
                hit_rect,
                ui.id().with(&node.id),
                egui::Sense::click_and_drag(),
            );
            if ghost_resp.clicked() {
                self.state.selected_node = Some(node.id.clone());
                self.state.selected_detail = Some(format!(
                    "Ghost Draft: {}\nScore: {:.2}",
                    node.id, node.score
                ));
                let _ = self.state.ui_state_machine.dispatch(UiEvent::StartEdit);
            }
            if ghost_resp.dragged() {
                let p = pos + ghost_resp.drag_delta();
                self.state
                    .graph_positions
                    .insert(node.id.clone(), (p.x, p.y));
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
            if let (Some(p1), Some(p2)) =
                (node_positions.get(&edge.from), node_positions.get(&edge.to))
            {
                painter.line_segment(
                    [*p1, *p2],
                    egui::Stroke::new(1.5, egui::Color32::from_gray(150)),
                );
            }
        }

        if response.clicked() && !ui.ui_contains_pointer() {
            self.state.selected_node = None;
            self.state.selected_detail = None;
        }
    }

    fn render_concept_layer(&mut self, ui: &mut egui::Ui) {
        ui.heading("Concept Layer");
        ui.label("システムのコンセプトやアイディアを自由に入力してください。この内容は設計の根幹となります。");
        ui.add_space(8.0);
        let resp = ui.add(
            egui::TextEdit::multiline(&mut self.state.concept_text)
                .hint_text("例: 新しい分散型SNSのアーキテクチャ案...")
                .desired_width(f32::INFINITY)
                .desired_rows(15),
        );
        if resp.changed() {
            // 必要に応じて状態更新
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui.button("🚀 このコンセプトから仕様を抽出する").clicked() {
                self.state.input_text = self.state.concept_text.clone();
                Controller::analyze_append(&mut self.session, &mut self.state);
            }
        });
    }

    fn render_spec_layer(&mut self, ui: &mut egui::Ui) {
        ui.heading("Specification Layer (L1)");
        ui.label("コンセプトから抽出された大枠の仕様リストです。");
        ui.add_space(8.0);

        if self.state.l1_units.is_empty() {
            ui.weak("仕様がまだ抽出されていません。コンセプト層で分析を実行してください。");
        } else {
            for unit in &self.state.l1_units {
                let l2_count = self
                    .state
                    .cards
                    .iter()
                    .filter(|c| c.parent_id == unit.id)
                    .count();
                ui.group(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(format!("L1-{}", unit.id.0)).strong());
                        ui.label(egui::RichText::new(format!("({} items)", l2_count)).weak());

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let score_color = if unit.ambiguity_score < 0.3 {
                                egui::Color32::LIGHT_GREEN
                            } else if unit.ambiguity_score < 0.6 {
                                egui::Color32::YELLOW
                            } else {
                                egui::Color32::LIGHT_RED
                            };
                            ui.label(
                                egui::RichText::new(format!(
                                    "Ambiguity: {:.2}",
                                    unit.ambiguity_score
                                ))
                                .color(score_color),
                            );
                        });
                    });
                    if let Some(obj) = &unit.objective {
                        ui.label(obj);
                    }
                });
            }
        }
    }

    fn render_item_layer(&mut self, ui: &mut egui::Ui) {
        ui.heading("Item Layer (L2)");
        ui.label("各仕様を具体化した設計項目カードです。");
        ui.add_space(8.0);

        let l1_units = self.state.l1_units.clone();
        if l1_units.is_empty() {
            ui.weak("親となる仕様（L1）が見つかりません。");
            return;
        }

        for l1 in l1_units {
            let child_cards: Vec<_> = self
                .state
                .cards
                .iter()
                .filter(|c| c.parent_id == l1.id)
                .cloned()
                .collect();
            if child_cards.is_empty() {
                continue;
            }

            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(format!(
                    "📜 L1: {}",
                    l1.objective.as_deref().unwrap_or("Untitled")
                ))
                .strong()
                .size(15.0),
            );
            ui.separator();

            for card in child_cards {
                let card_key = format!("L2-{}", card.id.0);
                let has_grounding = !card.grounding_data.is_empty();

                ui.add_space(4.0);
                ui.scope(|ui| {
                    let frame = egui::Frame::group(ui.style())
                        .fill(egui::Color32::from_gray(35))
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(60)));

                    frame.show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&card_key).strong().size(14.0));

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if has_grounding {
                                        ui.label(
                                            egui::RichText::new("● Grounded")
                                                .color(egui::Color32::LIGHT_GREEN)
                                                .size(11.0),
                                        );
                                    } else {
                                        ui.label(
                                            egui::RichText::new("○ Need Info")
                                                .color(egui::Color32::GOLD)
                                                .size(11.0),
                                        );
                                    }
                                },
                            );
                        });

                        if !card.metrics.is_empty() {
                            ui.weak(format!("Metrics: {}", card.metrics.join(", ")));
                        }

                        let mut edited_text = self
                            .state
                            .card_edit_buffers
                            .get(&card_key)
                            .cloned()
                            .unwrap_or_default();
                        let text_resp = ui.add(
                            egui::TextEdit::multiline(&mut edited_text)
                                .hint_text("具体的な仕様を入力...")
                                .desired_rows(1)
                                .desired_width(f32::INFINITY),
                        );

                        if text_resp.changed() {
                            self.state
                                .card_edit_buffers
                                .insert(card_key.clone(), edited_text.clone());
                        }

                        ui.horizontal(|ui| {
                            if ui.button("💾 Save").clicked() && !edited_text.trim().is_empty() {
                                Controller::refine_card(
                                    &mut self.session,
                                    &mut self.state,
                                    &card_key,
                                    &edited_text,
                                );
                            }
                            if ui.button("🌐 Grounding").clicked() {
                                Controller::ground_card(
                                    &mut self.session,
                                    &mut self.state,
                                    &card_key,
                                );
                            }
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.button("🛠️ Build Unit").clicked() {
                                        // 未来機能
                                    }
                                },
                            );
                        });
                    });
                });
            }
        }
    }

    fn render_advisor(&mut self, ui: &mut egui::Ui) {
        ui.heading("Knowledge Advisor");
        if self.state.missing_info.is_empty() {
            ui.label("設計の具体性は現在十分です。");
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
                            if ui.button("回答").clicked() {
                                self.state.input_text = format!("{} について: ", info.prompt);
                            }
                        });
                    });
                    ui.label(&info.prompt);
                });
            }
        }

        ui.separator();
        ui.heading("Design Improvements");
        if self.state.drafts.is_empty() {
            ui.label("改善案はありません。");
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
                        Controller::adopt_draft(
                            &mut self.session,
                            &mut self.state,
                            &draft.draft_id,
                        );
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
                ui.label(format!(
                    "State: {:?}",
                    self.state.ui_state_machine.current_state()
                ));

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
                let text_resp = ui.add(
                    egui::TextEdit::singleline(&mut self.state.input_text)
                        .hint_text("追加の要件や回答を入力..."),
                );
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

        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.small(format!("Storage: {}", self.session.store_path.display()));
                ui.separator();
                ui.small(format!(
                    "Units: L1:{}/L2:{}",
                    self.state.l1_units.len(),
                    self.state.cards.len()
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.small("DesignBrainModel G3A - Ready");
                });
            });
        });

        egui::SidePanel::left("nav_panel")
            .resizable(false)
            .default_width(120.0)
            .show(ctx, |ui| {
                ui.vertical_centered_justified(|ui| {
                    ui.add_space(10.0);
                    ui.heading("Design Mode");
                    ui.separator();

                    if ui
                        .selectable_label(
                            self.state.current_tab == crate::state::DesignTab::Concept,
                            "💡 Concept",
                        )
                        .clicked()
                    {
                        self.state.current_tab = crate::state::DesignTab::Concept;
                    }
                    if ui
                        .selectable_label(
                            self.state.current_tab == crate::state::DesignTab::Specification,
                            "📜 Spec (L1)",
                        )
                        .clicked()
                    {
                        self.state.current_tab = crate::state::DesignTab::Specification;
                    }
                    if ui
                        .selectable_label(
                            self.state.current_tab == crate::state::DesignTab::Item,
                            "🗂️ Item (L2)",
                        )
                        .clicked()
                    {
                        self.state.current_tab = crate::state::DesignTab::Item;
                    }
                });
            });

        egui::SidePanel::right("advisor_panel")
            .min_width(300.0)
            .show(ctx, |ui| {
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
                        match self.state.current_tab {
                            crate::state::DesignTab::Concept => self.render_concept_layer(ui),
                            crate::state::DesignTab::Specification => self.render_spec_layer(ui),
                            crate::state::DesignTab::Item => self.render_item_layer(ui),
                        }

                        if let Some(exp) = &self.state.explanation {
                            ui.separator();
                            ui.heading("Overview Summary");
                            ui.group(|ui| {
                                let source_text = if self.state.concept_text.trim().is_empty() {
                                    self.state.input_text.as_str()
                                } else {
                                    self.state.concept_text.as_str()
                                };
                                render_explanation(ui, exp, source_text, debug_mode_enabled());
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
            fonts.font_data.insert(
                "japanese_font".to_owned(),
                egui::FontData::from_owned(font_data),
            );
            font_loaded = true;
            break;
        }
    }
    if font_loaded {
        fonts
            .families
            .entry(egui::FontFamily::Proportional)
            .or_default()
            .insert(0, "japanese_font".to_owned());
        fonts
            .families
            .entry(egui::FontFamily::Monospace)
            .or_default()
            .push("japanese_font".to_owned());
        ctx.set_fonts(fonts);
    }
}
