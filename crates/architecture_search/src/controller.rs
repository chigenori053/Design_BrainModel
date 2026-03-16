use crate::{SearchConfig, SearchState};

pub trait SearchController {
    fn search(&self, initial_state: SearchState) -> Vec<SearchState>;
    fn config(&self) -> &SearchConfig;
}
