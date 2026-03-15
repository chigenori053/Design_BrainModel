use crate::SearchState;

pub trait SearchController {
    fn search(&self, initial_state: SearchState) -> Vec<SearchState>;
}
