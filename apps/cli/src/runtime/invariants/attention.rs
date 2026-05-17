use crate::runtime::cognitive_orchestration::AttentionState;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttentionDecay {
    pub decay_factor: f64,
}

impl AttentionDecay {
    pub fn bounded(decay_factor: f64) -> Self {
        Self {
            decay_factor: decay_factor.clamp(0.0, 1.0),
        }
    }
}

pub fn decay_attention(state: &AttentionState, decay: AttentionDecay) -> AttentionState {
    let mut focused_goals = state.focused_goals.clone();
    focused_goals.sort();
    focused_goals.dedup();
    focused_goals.truncate(3);

    let mut suppressed_contexts = state.suppressed_contexts.clone();
    suppressed_contexts.sort();
    suppressed_contexts.dedup();

    AttentionState {
        focused_goals,
        suppressed_contexts,
        attention_score: (state.attention_score * decay.decay_factor).clamp(0.0, 1.0),
    }
}

pub struct AttentionInvariantSuite;

impl AttentionInvariantSuite {
    pub fn assert_bounded_attention(state: &AttentionState) {
        assert!(state.attention_score >= 0.0);
        assert!(state.attention_score <= 1.0);
        assert!(state.focused_goals.len() <= 3);
    }

    pub fn assert_decay_decreases(before: &AttentionState, after: &AttentionState) {
        assert!(after.attention_score <= before.attention_score);
    }
}
