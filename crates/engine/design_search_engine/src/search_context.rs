use architecture_knowledge::ArchitectureKnowledge;
use architecture_memory::ArchitectureMemory;

use crate::{SearchConfig, SearchState};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchContext {
    pub knowledge: ArchitectureKnowledge,
    pub memory: ArchitectureMemory,
}

impl SearchContext {
    pub fn constrained_candidates(&self, config: &SearchConfig) -> usize {
        if !self.memory.patterns.is_empty() && !self.knowledge.patterns.is_empty() {
            (config.max_candidates / 4).max(1)
        } else if !self.memory.patterns.is_empty() {
            (config.max_candidates / 2).max(1)
        } else if !self.knowledge.patterns.is_empty() {
            ((config.max_candidates as f64 * 0.7).round() as usize).max(1)
        } else {
            config.max_candidates.max(1)
        }
    }

    pub fn constrained_beam_width(&self, config: &SearchConfig) -> usize {
        if !self.memory.patterns.is_empty() && !self.knowledge.patterns.is_empty() {
            (config.beam_width / 2).max(1)
        } else if !self.memory.patterns.is_empty() {
            ((config.beam_width as f64 * 0.75).round() as usize).max(1)
        } else {
            config.beam_width.max(1)
        }
    }

    pub fn score_bias(&self, state: &SearchState) -> f64 {
        let mut bias = 0.0;
        let action_name = state
            .source_action
            .as_ref()
            .map(|action| format!("{:?}", action).to_ascii_lowercase())
            .unwrap_or_default();
        if self
            .knowledge
            .patterns
            .iter()
            .any(|pattern| pattern.name.to_ascii_lowercase().contains("layered"))
            && action_name.contains("connectdependency")
        {
            bias += 0.05;
        }
        if !self.memory.patterns.is_empty() && action_name.contains("adddesignunit") {
            bias += 0.05;
        }
        bias
    }
}
