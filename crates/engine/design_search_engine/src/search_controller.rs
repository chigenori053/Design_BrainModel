use memory_space_core::RecallResult;
use world_model_core::WorldState;

use crate::search_config::SearchConfig;
use crate::search_state::SearchState;

/// Controls the multi-step architecture search.
pub trait SearchController {
    fn search(
        &self,
        initial_state: WorldState,
        recall: Option<&RecallResult>,
        config: &SearchConfig,
    ) -> Vec<SearchState>;
}
