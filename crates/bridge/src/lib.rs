use std::collections::BTreeSet;

use architecture_ir::stable_v03::ArchitectureGraph;
use contracts::{
    Context, Goal, Hypothesis, HypothesisId, Intent, ReasoningInput, ScoreParts,
    SemanticRepresentation, State, request_id_for, semantic_hash_for_text, state_hash_for_graph,
};
use world_model::stable_v03::IntentState;

#[cfg(feature = "contract_strict")]
macro_rules! forbid_legacy {
    () => {
        const _: () = ();
    };
}

#[cfg(not(feature = "contract_strict"))]
macro_rules! forbid_legacy {
    () => {};
}

pub mod legacy {
    use architecture_ir::stable_v03::ArchitectureGraph;
    use contracts::SemanticHash;

    #[derive(Clone, Debug)]
    pub struct LegacyHypothesis {
        pub id: usize,
        pub state: ArchitectureGraph,
        pub parent: Option<usize>,
        pub depth: usize,
        pub score: f32,
        pub relevance: f32,
        pub goal_distance: f32,
        pub constraint: f32,
        pub memory: f32,
        pub semantic_hash: SemanticHash,
    }
}

pub fn reasoning_input_from_intent(
    intent: &IntentState,
    extra_tokens: &[String],
    context: Context,
) -> ReasoningInput {
    forbid_legacy!();
    let mut labels = intent
        .tokens
        .iter()
        .map(|token| token.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    for token in extra_tokens {
        labels.insert(token.to_ascii_lowercase());
    }
    let intents = labels
        .iter()
        .cloned()
        .map(|label| Intent { label })
        .collect::<Vec<_>>();
    let semantic = SemanticRepresentation::new(Vec::new(), intents);
    let goal = Goal {
        target: intent.raw.clone(),
        required_intents: labels.into_iter().collect(),
    };
    let request_id = request_id_for(&goal.target, &semantic.hash);
    ReasoningInput {
        semantic,
        context,
        goal,
        request_id,
        memory_candidates: Vec::new(),
    }
}

impl From<legacy::LegacyHypothesis> for Hypothesis {
    fn from(value: legacy::LegacyHypothesis) -> Self {
        let score = value.score.clamp(0.0, 1.0);
        Hypothesis {
            id: HypothesisId(value.id),
            state: State {
                architecture: value.state.clone(),
            },
            parent: value.parent.map(HypothesisId),
            depth: value.depth,
            score,
            score_parts: ScoreParts {
                relevance: value.relevance.clamp(0.0, 1.0),
                goal_distance: value.goal_distance.clamp(0.0, 1.0),
                constraint: value.constraint.clamp(0.0, 1.0),
                memory: value.memory.clamp(0.0, 1.0),
            },
            state_hash: state_hash_for_graph(&value.state),
            semantic_hash: value.semantic_hash,
        }
    }
}

pub fn contract_hypothesis_from_graph(
    graph: ArchitectureGraph,
    semantic_seed: &str,
) -> Hypothesis {
    Hypothesis {
        id: HypothesisId(0),
        state: State {
            architecture: graph.clone(),
        },
        parent: None,
        depth: 0,
        score: 0.0,
        score_parts: ScoreParts {
            relevance: 0.0,
            goal_distance: 0.0,
            constraint: 1.0,
            memory: 0.0,
        },
        state_hash: state_hash_for_graph(&graph),
        semantic_hash: semantic_hash_for_text(semantic_seed),
    }
}
