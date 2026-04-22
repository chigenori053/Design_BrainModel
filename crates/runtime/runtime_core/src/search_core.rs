/// Pure search algorithm types — no external dependencies.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SearchState {
    pub score: f64,
    pub depth: usize,
    pub branch_id: u64,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            score: 0.0,
            depth: 0,
            branch_id: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BranchNode {
    pub id: u64,
    pub parent_id: Option<u64>,
    pub score: f64,
    pub depth: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchSummary {
    pub score: f64,
    pub branch_id: u64,
    pub depth: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum SearchStatus {
    #[default]
    NotStarted,
    Running,
    Completed(SearchSummary),
}

impl SearchStatus {
    pub fn is_complete(&self) -> bool {
        matches!(self, SearchStatus::Completed(_))
    }

    pub fn score(&self) -> Option<f64> {
        match self {
            SearchStatus::Completed(s) => Some(s.score),
            _ => None,
        }
    }
}

pub fn rank_by_score(states: &mut Vec<SearchState>) {
    states.sort_by(|a, b| b.score.total_cmp(&a.score));
}

pub fn prune_candidates(states: &mut Vec<SearchState>, max_width: usize) {
    if states.len() > max_width {
        rank_by_score(states);
        states.truncate(max_width);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_status_defaults_to_not_started() {
        assert_eq!(SearchStatus::default(), SearchStatus::NotStarted);
    }

    #[test]
    fn completed_status_exposes_score() {
        let status = SearchStatus::Completed(SearchSummary {
            score: 0.75,
            branch_id: 1,
            depth: 3,
        });
        assert!(status.is_complete());
        assert_eq!(status.score(), Some(0.75));
    }

    #[test]
    fn prune_candidates_keeps_top_n_by_score() {
        let mut states = vec![
            SearchState { score: 0.3, depth: 1, branch_id: 1 },
            SearchState { score: 0.9, depth: 1, branch_id: 2 },
            SearchState { score: 0.5, depth: 1, branch_id: 3 },
        ];
        prune_candidates(&mut states, 2);
        assert_eq!(states.len(), 2);
        assert_eq!(states[0].branch_id, 2);
    }
}
