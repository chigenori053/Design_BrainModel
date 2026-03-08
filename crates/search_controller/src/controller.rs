use concept_engine::ConceptId;
use memory_space_api::ConceptRecallHit;
use memory_space_complex::normalize;
use memory_space_core::Complex64;

use crate::beam_search::run_beam_search;
use crate::config::SearchConfig;
use crate::heuristic::{HeuristicSignal, score};
use crate::search_state::SearchState;

#[derive(Clone, Debug)]
pub struct SearchController {
    config: SearchConfig,
}

impl SearchController {
    pub fn new(config: SearchConfig) -> Self {
        Self { config }
    }

    pub fn config(&self) -> SearchConfig {
        self.config
    }

    pub fn search(
        &self,
        initial: SearchState,
        concepts: &[ConceptId],
        memory: &[ConceptRecallHit],
        intent_edges: usize,
    ) -> Vec<SearchState> {
        let concepts = concepts.to_vec();
        let memory_signal = if memory.is_empty() {
            0.0
        } else {
            memory.iter().map(|m| f64::from(m.score)).sum::<f64>() / memory.len() as f64
        };
        let concept_signal = (concepts.len() as f64 / 10.0).clamp(0.0, 1.0);
        let intent_signal = (intent_edges as f64 / 10.0).clamp(0.0, 1.0);

        run_beam_search(
            initial,
            self.config,
            move |state| expand_state(state, &concepts),
            move |_| {
                score(HeuristicSignal {
                    memory_resonance: memory_signal,
                    concept_match: concept_signal,
                    intent_alignment: intent_signal,
                })
            },
        )
    }
}

fn expand_state(state: &SearchState, concepts: &[ConceptId]) -> Vec<SearchState> {
    if concepts.is_empty() {
        return Vec::new();
    }

    concepts
        .iter()
        .map(|concept| {
            let mut next = state.clone();
            next.depth = next.depth.saturating_add(1);
            let c = concept_to_complex(*concept);
            if next.state_vector.data.is_empty() {
                next.state_vector.data.push(c);
            } else {
                let idx = next.depth % next.state_vector.data.len();
                next.state_vector.data[idx] += c;
            }
            normalize(&mut next.state_vector);
            next
        })
        .collect()
}

fn concept_to_complex(concept: ConceptId) -> Complex64 {
    let lo = (concept.0 & 0xFF) as f32 / 255.0;
    let hi = ((concept.0 >> 8) & 0xFF) as f32 / 255.0;
    Complex64::new(lo, hi)
}
