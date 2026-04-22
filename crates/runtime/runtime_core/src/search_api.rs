/// Public search API — single entry point for search orchestration.
///
/// Dependency direction: search_api → search_runtime → search_domain → search_core
///                                                                    → execution_core (plan)

use execution_core::ExecutionPlan;

use crate::search_core::SearchStatus;
use crate::search_domain::SearchInput;
use crate::search_runtime::SearchPipeline;

#[derive(Debug, Clone, Default)]
pub struct SearchResult {
    pub status: SearchStatus,
    /// Reserved for Phase D — populated when execution planning is enabled.
    pub execution_plan: Option<ExecutionPlan>,
}

impl SearchResult {
    pub fn score(&self) -> Option<f64> {
        self.status.score()
    }

    pub fn is_complete(&self) -> bool {
        self.status.is_complete()
    }
}

/// Main search entry point.
/// Orchestrates search_core + search_domain + search_runtime → ExecutionPlan.
pub fn search(input: SearchInput) -> SearchResult {
    let pipeline = SearchPipeline::default();
    SearchResult {
        status: pipeline.run(&input),
        execution_plan: None,
    }
}
