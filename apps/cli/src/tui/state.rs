use super::model::{HypothesisViewModel, MemoryCandidateViewModel, UiPayload};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivePanel {
    Trace,
    Hypothesis,
    Memory,
}

impl ActivePanel {
    pub fn next(self) -> Self {
        match self {
            Self::Trace => Self::Hypothesis,
            Self::Hypothesis => Self::Memory,
            Self::Memory => Self::Trace,
        }
    }
}

pub struct TuiState {
    pub payload: UiPayload,
    /// Index into `payload.trace.steps`.
    pub selected_step: usize,
    /// Id of the currently selected hypothesis.
    pub selected_hypothesis: Option<usize>,
    /// Index into `payload.memory` for Memory panel scroll.
    pub memory_scroll: usize,
    pub active_panel: ActivePanel,
}

impl TuiState {
    pub fn new(payload: UiPayload) -> Self {
        let selected_hypothesis = payload.selected;
        Self {
            payload,
            selected_step: 0,
            selected_hypothesis,
            memory_scroll: 0,
            active_panel: ActivePanel::Trace,
        }
    }

    // ── Selection accessors ──────────────────────────────────────────────────

    /// Depth of the currently selected trace step.
    pub fn selected_depth(&self) -> Option<usize> {
        self.payload.trace.steps.get(self.selected_step).map(|s| s.depth)
    }

    /// Ids of all hypotheses whose depth matches `depth`.
    pub fn hypothesis_ids_at_depth(&self, depth: usize) -> Vec<usize> {
        self.payload
            .hypotheses
            .iter()
            .filter(|h| h.depth == depth)
            .map(|h| h.id)
            .collect()
    }

    /// Data for the currently selected hypothesis.
    pub fn selected_hypothesis_data(&self) -> Option<&HypothesisViewModel> {
        let id = self.selected_hypothesis?;
        self.payload.hypotheses.iter().find(|h| h.id == id)
    }

    /// Memory candidates relevant to the selected hypothesis depth.
    /// For now returns all candidates (future: filter by step origin).
    pub fn visible_memory(&self) -> &[MemoryCandidateViewModel] {
        &self.payload.memory
    }

    // ── Navigation ───────────────────────────────────────────────────────────

    pub fn move_up(&mut self) {
        match self.active_panel {
            ActivePanel::Trace => {
                if self.selected_step > 0 {
                    self.selected_step -= 1;
                    self.sync_hypothesis_to_step();
                }
            }
            ActivePanel::Hypothesis => {
                let ids = self.visible_hypothesis_ids();
                if let Some(pos) = self.hypothesis_list_pos(&ids) {
                    if pos > 0 {
                        self.selected_hypothesis = Some(ids[pos - 1]);
                        self.sync_step_to_hypothesis();
                    }
                }
            }
            ActivePanel::Memory => {
                self.memory_scroll = self.memory_scroll.saturating_sub(1);
            }
        }
    }

    pub fn move_down(&mut self) {
        match self.active_panel {
            ActivePanel::Trace => {
                if self.selected_step + 1 < self.payload.trace.steps.len() {
                    self.selected_step += 1;
                    self.sync_hypothesis_to_step();
                }
            }
            ActivePanel::Hypothesis => {
                let ids = self.visible_hypothesis_ids();
                if let Some(pos) = self.hypothesis_list_pos(&ids) {
                    if pos + 1 < ids.len() {
                        self.selected_hypothesis = Some(ids[pos + 1]);
                        self.sync_step_to_hypothesis();
                    }
                }
            }
            ActivePanel::Memory => {
                let max = self.payload.memory.len().saturating_sub(1);
                if self.memory_scroll < max {
                    self.memory_scroll += 1;
                }
            }
        }
    }

    pub fn toggle_panel(&mut self) {
        self.active_panel = self.active_panel.next();
    }

    // ── Sync ─────────────────────────────────────────────────────────────────

    /// Trace step selected → pick first hypothesis at that depth.
    fn sync_hypothesis_to_step(&mut self) {
        if let Some(depth) = self.selected_depth() {
            let ids = self.hypothesis_ids_at_depth(depth);
            if !ids.is_empty() {
                self.selected_hypothesis = Some(ids[0]);
            }
        }
    }

    /// Hypothesis selected → move trace step to matching depth.
    fn sync_step_to_hypothesis(&mut self) {
        if let Some(id) = self.selected_hypothesis {
            if let Some(h) = self.payload.hypotheses.iter().find(|h| h.id == id) {
                let depth = h.depth;
                if let Some(pos) = self.payload.trace.steps.iter().position(|s| s.depth == depth) {
                    self.selected_step = pos;
                }
            }
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    pub fn visible_hypothesis_ids(&self) -> Vec<usize> {
        self.payload.hypotheses.iter().map(|h| h.id).collect()
    }

    fn hypothesis_list_pos(&self, ids: &[usize]) -> Option<usize> {
        self.selected_hypothesis
            .and_then(|id| ids.iter().position(|&v| v == id))
    }

    /// List-widget selection index for the hypothesis panel.
    pub fn hypothesis_list_index(&self) -> Option<usize> {
        let ids = self.visible_hypothesis_ids();
        self.hypothesis_list_pos(&ids)
    }
}
