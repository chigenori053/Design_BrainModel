use crate::state::{AppState, CausalGraph, GraphNode, GraphNodeType, GraphEdge, GraphEdgeType, UiEvent};
use crate::session::{GuiSession, HistoryEntry};
use design_reasoning::{Explanation, LanguageEngine, LanguageStateV2, TemplateId};
use hybrid_vm::{ConceptId, ConceptUnitV2};

pub struct Controller;

impl Controller {
    /// 既存のコンテキストを維持したまま、新規テキストを追記して分析する
    pub fn analyze_append(session: &mut GuiSession, state: &mut AppState) {
        let _ = state.ui_state_machine.dispatch(UiEvent::Submit);

        match session.vm.analyze_incremental(&state.input_text) {
            Ok(_) => {
                Self::refresh_state(session, state);
                state.input_text.clear(); // 入力欄をクリアして次の入力を促す
                let _ = state.ui_state_machine.dispatch(UiEvent::AnalysisSucceeded);
            }
            Err(err) => {
                state.last_error = Some(format!("Incremental Analysis Error: {:?}", err));
                let _ = state.ui_state_machine.dispatch(UiEvent::AnalysisFailed);
            }
        }
    }

    /// セッションを完全に初期化する
    pub fn clear_session(session: &mut GuiSession, state: &mut AppState) {
        let _ = session.vm.clear_context();
        state.l1_units.clear();
        state.l2_units.clear();
        state.graph = CausalGraph::default();
        state.graph_positions.clear();
        state.explanation = None;
        state.snapshot = None;
        state.edge_builder_from = None;
        state.cards.clear();
        state.card_edit_buffers.clear();
        state.missing_info.clear();
        state.drafts.clear();
        let _ = state.ui_state_machine.dispatch(UiEvent::Reset);
        session.update_modified();
    }

    pub fn adopt_draft(session: &mut GuiSession, state: &mut AppState, draft_id: &str) {
        if session.vm.commit_draft(draft_id).is_ok() {
            Self::refresh_state(session, state);
        } else {
            state.last_error = Some(format!("Draft adopt failed: {draft_id}"));
        }
    }

    /// 内部VMの状態をAppStateに反映させる
    fn refresh_state(session: &mut GuiSession, state: &mut AppState) {
        let _ = session.vm.rebuild_l2_from_l1_v2();
        let l1 = session.vm.all_l1_units_v2().unwrap_or_default();
        let l2 = session.vm.project_phase_a_v2().unwrap_or_default();
        let snap = session.vm.snapshot_v2().ok();
        let missing = session.vm.extract_missing_information().unwrap_or_default();
        let drafts = session.vm.generate_drafts().unwrap_or_default();
        let cards = session.vm.list_l2_details().unwrap_or_default();

        if let Some(s) = snap {
            let s_val: hybrid_vm::MeaningLayerSnapshotV2 = s.clone();
            session.history.push(HistoryEntry {
                l1_units: l1.clone(),
                l2_units: l2.clone(),
                snapshot: s_val,
            });
            state.l1_units = l1;
            state.l2_units = l2;
            state.snapshot = Some(s);
            state.graph = Self::generate_graph(&state.l1_units, &state.l2_units, &drafts);
            state.graph_positions.retain(|id, _| state.graph.nodes.iter().any(|n| n.id == *id));
            state.missing_info = missing;
            state.drafts = drafts;
            state.cards = cards;
            state.card_edit_buffers.retain(|id, _| state.cards.iter().any(|c| format!("L2-{}", c.id.0) == *id));
        }
        
        state.last_error = None;
        session.update_modified();
        Self::explain(session, state);
    }

    fn generate_graph(
        l1_units: &[hybrid_vm::SemanticUnitL1V2],
        l2_units: &[ConceptUnitV2],
        drafts: &[hybrid_vm::DesignDraft],
    ) -> CausalGraph {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        for l1 in l1_units {
            nodes.push(GraphNode {
                id: format!("L1-{}", l1.id.0),
                node_type: GraphNodeType::L1,
                label: l1.objective.clone().unwrap_or_else(|| format!("L1-{}", l1.id.0)),
                score: l1.ambiguity_score,
            });
        }

        for l2 in l2_units {
            nodes.push(GraphNode {
                id: format!("L2-{}", l2.id.0),
                node_type: GraphNodeType::L2,
                label: format!("Concept-{}", l2.id.0),
                score: l2.stability_score,
            });

            for link in &l2.causal_links {
                edges.push(GraphEdge {
                    from: format!("L1-{}", link.from.0),
                    to: format!("L2-{}", l2.id.0),
                    edge_type: GraphEdgeType::Mapping,
                    weight: Some(link.weight),
                });
            }
        }

        for draft in drafts {
            for unit in &draft.added_units {
                let ghost_id = format!("GHOST-{}", draft.draft_id);
                nodes.push(GraphNode {
                    id: ghost_id,
                    node_type: GraphNodeType::Ghost,
                    label: unit.objective.clone().unwrap_or_else(|| draft.draft_id.clone()),
                    score: (0.5 + draft.stability_impact).clamp(0.0, 1.0),
                });
            }
        }

        CausalGraph { nodes, edges }
    }

    pub fn explain(_session: &mut GuiSession, state: &mut AppState) {
        let objective = state.l1_units.iter().find_map(|u| u.objective.clone());
        let requirement_count = state.l2_units.iter().map(|c| c.derived_requirements.len()).sum::<usize>();
        let stability = mean_stability(&state.l2_units);
        let ambiguity = mean_ambiguity(&state.l1_units);

        let lang_state = LanguageStateV2 {
            selected_objective: objective.clone(),
            requirement_count,
            stability_score: stability,
            ambiguity_score: ambiguity,
        };
        let language = LanguageEngine::new();
        let h = language.build_h_state(&lang_state);
        let template = language.select_template(&h).unwrap_or(TemplateId::Fallback);

        let summary = format!(
            "設計目標: {}\n{}\n\n(補足: 構造安定性={}, 曖昧性={}, 派生要件数={}件)",
            objective.as_deref().unwrap_or("未指定"),
            template.as_description(),
            stability_label(stability),
            ambiguity_label(ambiguity),
            requirement_count
        );
        let detail = format!("template={:?}, stability={:.3}, ambiguity={:.3}", template, stability, ambiguity);

        state.explanation = Some(Explanation { summary, detail });
    }

    pub fn undo(session: &mut GuiSession, state: &mut AppState) {
        if let Some(entry) = session.history.undo() {
            state.l1_units = entry.l1_units.clone();
            state.l2_units = entry.l2_units.clone();
            state.snapshot = Some(entry.snapshot.clone());
            state.graph = Self::generate_graph(&state.l1_units, &state.l2_units, &state.drafts);
            session.update_modified();
            Self::explain(session, state);
        }
    }

    pub fn redo(session: &mut GuiSession, state: &mut AppState) {
        if let Some(entry) = session.history.redo() {
            state.l1_units = entry.l1_units.clone();
            state.l2_units = entry.l2_units.clone();
            state.snapshot = Some(entry.snapshot.clone());
            state.graph = Self::generate_graph(&state.l1_units, &state.l2_units, &state.drafts);
            session.update_modified();
            Self::explain(session, state);
        }
    }

    #[allow(dead_code)]
    pub fn remove_selected_node(state: &mut AppState) {
        let Some(selected) = state.selected_node.clone() else {
            return;
        };
        state.graph.nodes.retain(|n| n.id != selected);
        state.graph.edges.retain(|e| e.from != selected && e.to != selected);
        state.graph_positions.remove(&selected);
        if state.edge_builder_from.as_ref() == Some(&selected) {
            state.edge_builder_from = None;
        }
        state.selected_node = None;
        state.selected_detail = None;
    }

    #[allow(dead_code)]
    pub fn begin_edge(state: &mut AppState) {
        state.edge_builder_from = state.selected_node.clone();
    }

    #[allow(dead_code)]
    pub fn connect_to_selected(state: &mut AppState) {
        let Some(from) = state.edge_builder_from.clone() else {
            return;
        };
        let Some(to) = state.selected_node.clone() else {
            return;
        };
        if from == to {
            return;
        }
        if state.graph.edges.iter().any(|e| e.from == from && e.to == to) {
            return;
        }
        state.graph.edges.push(GraphEdge {
            from,
            to,
            edge_type: GraphEdgeType::Causal,
            weight: None,
        });
        state.edge_builder_from = None;
    }

    pub fn refine_card(session: &mut GuiSession, state: &mut AppState, card_id: &str, text: &str) {
        let raw = card_id.strip_prefix("L2-").unwrap_or(card_id);
        let Ok(id) = raw.parse::<u64>() else {
            state.last_error = Some(format!("invalid card id: {card_id}"));
            return;
        };
        if let Err(err) = session.vm.refine_l2_detail(ConceptId(id), text) {
            state.last_error = Some(format!("Card refine failed: {err:?}"));
            return;
        }
        Self::refresh_state(session, state);
    }

    pub fn ground_card(session: &mut GuiSession, state: &mut AppState, card_id: &str) {
        let raw = card_id.strip_prefix("L2-").unwrap_or(card_id);
        let Ok(id) = raw.parse::<u64>() else {
            state.last_error = Some(format!("invalid card id: {card_id}"));
            return;
        };
        let l2 = ConceptId(id);
        match session.vm.card_has_knowledge_gap(l2) {
            Ok(true) => {
                let query = format!("grounding for {}", card_id);
                if let Err(err) = session.vm.run_grounding_search(l2, &query) {
                    state.last_error = Some(format!("Card grounding failed: {err:?}"));
                    return;
                }
                Self::refresh_state(session, state);
            }
            Ok(false) => {
                state.last_error = Some("No grounding gap detected for this card".to_string());
            }
            Err(err) => state.last_error = Some(format!("Card grounding failed: {err:?}")),
        }
    }
}

fn mean_stability(l2_units: &[ConceptUnitV2]) -> f64 {
    if l2_units.is_empty() { 0.0 }
    else { l2_units.iter().map(|u| u.stability_score).sum::<f64>() / l2_units.len() as f64 }
}

fn mean_ambiguity(l1_units: &[hybrid_vm::SemanticUnitL1V2]) -> f64 {
    if l1_units.is_empty() { 1.0 }
    else { l1_units.iter().map(|u| u.ambiguity_score).sum::<f64>() / l1_units.len() as f64 }
}

fn stability_label(score: f64) -> &'static str {
    if score > 0.85 { "安定" } else if score >= 0.6 { "概ね安定" } else { "不安定" }
}

fn ambiguity_label(score: f64) -> &'static str {
    if score > 0.7 { "不明確" } else if score >= 0.4 { "部分的に不明確" } else { "明確" }
}

#[cfg(test)]
mod tests {
    use super::Controller;
    use crate::state::{AppState, GraphEdge, GraphEdgeType, GraphNode, GraphNodeType};

    #[test]
    fn remove_selected_node_drops_incident_edges() {
        let mut state = AppState::default();
        state.graph.nodes = vec![
            GraphNode { id: "A".to_string(), node_type: GraphNodeType::L1, label: "A".to_string(), score: 0.2 },
            GraphNode { id: "B".to_string(), node_type: GraphNodeType::L2, label: "B".to_string(), score: 0.8 },
        ];
        state.graph.edges = vec![GraphEdge {
            from: "A".to_string(),
            to: "B".to_string(),
            edge_type: GraphEdgeType::Mapping,
            weight: Some(0.5),
        }];
        state.selected_node = Some("A".to_string());

        Controller::remove_selected_node(&mut state);

        assert!(state.graph.nodes.iter().all(|n| n.id != "A"));
        assert!(state.graph.edges.is_empty());
    }

    #[test]
    fn connect_to_selected_avoids_duplicates_and_self_loop() {
        let mut state = AppState::default();
        state.graph.nodes = vec![
            GraphNode { id: "A".to_string(), node_type: GraphNodeType::L1, label: "A".to_string(), score: 0.2 },
            GraphNode { id: "B".to_string(), node_type: GraphNodeType::L2, label: "B".to_string(), score: 0.8 },
        ];

        state.selected_node = Some("A".to_string());
        Controller::begin_edge(&mut state);
        state.selected_node = Some("B".to_string());
        Controller::connect_to_selected(&mut state);
        assert_eq!(state.graph.edges.len(), 1);

        state.edge_builder_from = Some("A".to_string());
        state.selected_node = Some("B".to_string());
        Controller::connect_to_selected(&mut state);
        assert_eq!(state.graph.edges.len(), 1);

        state.edge_builder_from = Some("A".to_string());
        state.selected_node = Some("A".to_string());
        Controller::connect_to_selected(&mut state);
        assert_eq!(state.graph.edges.len(), 1);
    }
}
