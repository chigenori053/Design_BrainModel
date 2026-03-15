use crate::{
    ArchitectureEvaluator, CandidateGenerator, ConstraintFilter, ParetoOptimizer, SearchController,
    SearchOutcome, SearchState, SearchTelemetry,
};

#[derive(Clone, Debug)]
pub struct BeamSearchController<G, F, E, P> {
    pub beam_width: usize,
    pub max_depth: usize,
    generator: G,
    filter: F,
    evaluator: E,
    pareto: P,
}

impl<G, F, E, P> BeamSearchController<G, F, E, P> {
    pub fn new(
        beam_width: usize,
        max_depth: usize,
        generator: G,
        filter: F,
        evaluator: E,
        pareto: P,
    ) -> Self {
        Self {
            beam_width,
            max_depth,
            generator,
            filter,
            evaluator,
            pareto,
        }
    }
}

impl<G, F, E, P> BeamSearchController<G, F, E, P>
where
    G: CandidateGenerator,
    F: ConstraintFilter,
    E: ArchitectureEvaluator,
    P: ParetoOptimizer,
{
    pub fn search_with_telemetry(&self, initial_state: SearchState) -> SearchOutcome {
        let mut telemetry = SearchTelemetry::default();
        let mut beam = vec![self.scored(initial_state)];

        for _ in 0..self.max_depth {
            telemetry.explored_states = telemetry.explored_states.saturating_add(beam.len());

            let mut generated = Vec::new();
            for state in &beam {
                generated.extend(self.generator.generate(state));
            }
            telemetry.generated_states = telemetry.generated_states.saturating_add(generated.len());

            if generated.is_empty() {
                break;
            }

            let filtered = self.filter.filter(generated);
            telemetry.pruned_states = telemetry.pruned_states.saturating_add(
                telemetry
                    .generated_states
                    .saturating_sub(filtered.len() + telemetry.pruned_states),
            );

            let mut evaluated = filtered
                .into_iter()
                .map(|state| self.scored(state))
                .collect::<Vec<_>>();
            evaluated.sort_by(|lhs, rhs| {
                rhs.score
                    .desirability()
                    .total_cmp(&lhs.score.desirability())
                    .then_with(|| lhs.depth.cmp(&rhs.depth))
                    .then_with(|| lhs.state_id.cmp(&rhs.state_id))
            });

            let mut frontier = self.pareto.select(evaluated);
            telemetry.pareto_states = frontier.len();
            frontier.truncate(self.beam_width.max(1));

            if frontier.is_empty() {
                break;
            }
            beam = frontier;
        }

        SearchOutcome {
            states: self.pareto.select(beam),
            telemetry,
        }
    }

    fn scored(&self, mut state: SearchState) -> SearchState {
        state.score = self.evaluator.evaluate(&state.architecture);
        state
    }
}

impl<G, F, E, P> SearchController for BeamSearchController<G, F, E, P>
where
    G: CandidateGenerator,
    F: ConstraintFilter,
    E: ArchitectureEvaluator,
    P: ParetoOptimizer,
{
    fn search(&self, initial_state: SearchState) -> Vec<SearchState> {
        self.search_with_telemetry(initial_state).states
    }
}
