/// Search orchestration — pipeline control and context integration.

use crate::search_core::{SearchStatus, SearchSummary};
use crate::search_domain::{SearchInput, compute_score};

pub struct SearchPipeline {
    pub max_depth: usize,
    pub beam_width: usize,
}

impl Default for SearchPipeline {
    fn default() -> Self {
        Self {
            max_depth: 4,
            beam_width: 3,
        }
    }
}

impl SearchPipeline {
    pub fn run(&self, input: &SearchInput) -> SearchStatus {
        let score = compute_score(input);
        if score.confidence == 0.0 {
            return SearchStatus::NotStarted;
        }
        SearchStatus::Completed(SearchSummary {
            score: score.value,
            branch_id: 1,
            depth: self.max_depth.min(input.concept_count),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_returns_not_started_for_empty_input() {
        let pipeline = SearchPipeline::default();
        let status = pipeline.run(&SearchInput::default());
        assert_eq!(status, SearchStatus::NotStarted);
    }

    #[test]
    fn pipeline_completes_with_concepts_present() {
        let pipeline = SearchPipeline::default();
        let status = pipeline.run(&SearchInput {
            concept_count: 3,
            memory_signal: 0.5,
            intent_edges: 2,
        });
        assert!(status.is_complete());
    }
}
