use causal_domain::CausalGraph;
use design_domain::{Architecture, Layer};
use semantic_domain::MeaningGraph;

#[derive(Clone, Debug, PartialEq)]
pub struct DesignExperience {
    pub semantic_context: MeaningGraph,
    pub inferred_semantics: MeaningGraph,
    pub architecture: Architecture,
    pub architecture_hash: u64,
    pub causal_graph: CausalGraph,
    pub dependency_edges: Vec<(u64, u64)>,
    pub layer_sequence: Vec<Layer>,
    pub score: f64,
    pub search_depth: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExperienceStore {
    high_score_threshold: f64,
    experiences: Vec<DesignExperience>,
}

impl ExperienceStore {
    pub fn new(high_score_threshold: f64) -> Self {
        Self {
            high_score_threshold,
            experiences: Vec::new(),
        }
    }

    pub fn update_experience(&mut self, experience: DesignExperience) -> bool {
        if experience.score < self.high_score_threshold {
            return false;
        }
        self.experiences.push(experience);
        self.experiences.sort_by(|lhs, rhs| {
            rhs.score
                .total_cmp(&lhs.score)
                .then_with(|| lhs.architecture_hash.cmp(&rhs.architecture_hash))
        });
        true
    }

    pub fn experiences(&self) -> &[DesignExperience] {
        &self.experiences
    }
}

impl Default for ExperienceStore {
    fn default() -> Self {
        Self::new(0.65)
    }
}
