use crate::state::AppState;
use crate::state::UiEvent;
use crate::session::{GuiSession, HistoryEntry};
use design_reasoning::{Explanation, LanguageEngine, LanguageStateV2, TemplateId};
use hybrid_vm::ConceptUnitV2;

pub struct Controller;

impl Controller {
    pub fn analyze(session: &mut GuiSession, state: &mut AppState) {
        let submit = state.ui_state_machine.dispatch(UiEvent::Submit);
        if !submit.applied {
            state.last_error = Some("Invalid transition: Submit".to_string());
            return;
        }

        match session.vm.analyze_text(&state.input_text) {
            Ok(_) => {
                let _ = session.vm.rebuild_l2_from_l1_v2();
                let l1 = session.vm.all_l1_units_v2().unwrap_or_default();
                let l2 = session.vm.project_phase_a_v2().unwrap_or_default();
                let snap = session.vm.snapshot_v2().ok();

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
                }
                
                state.last_error = None;
                let _ = state.ui_state_machine.dispatch(UiEvent::AnalysisSucceeded);
                session.update_modified();
                Self::explain(session, state);
            }
            Err(err) => {
                let _ = state.ui_state_machine.dispatch(UiEvent::AnalysisFailed);
                state.last_error = Some(format!("Analysis Error: {:?}", err));
            }
        }
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
            "設計目標: {}\n派生要件数: {}\n構造安定性: {}\n曖昧性: {}",
            objective.as_deref().unwrap_or("未指定"),
            requirement_count,
            stability_label(stability),
            ambiguity_label(ambiguity)
        );
        let detail = format!("template={:?}, stability={:.3}, ambiguity={:.3}", template, stability, ambiguity);

        state.explanation = Some(Explanation { summary, detail });
    }

    pub fn undo(session: &mut GuiSession, state: &mut AppState) {
        if let Some(entry) = session.history.undo() {
            state.l1_units = entry.l1_units.clone();
            state.l2_units = entry.l2_units.clone();
            state.snapshot = Some(entry.snapshot.clone());
            session.update_modified();
            Self::explain(session, state);
        }
    }

    pub fn redo(session: &mut GuiSession, state: &mut AppState) {
        if let Some(entry) = session.history.redo() {
            state.l1_units = entry.l1_units.clone();
            state.l2_units = entry.l2_units.clone();
            state.snapshot = Some(entry.snapshot.clone());
            session.update_modified();
            Self::explain(session, state);
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
