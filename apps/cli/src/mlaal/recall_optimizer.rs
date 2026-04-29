use super::adaptive_policy::AdaptivePolicy;
use super::episode_memory::EpisodeMemoryStore;
use super::episode_schema::{EpisodeRecord, RecallResult};
use super::memory_bridge::MemoryBridge;
use super::planner::{CognitiveContext, PlanningConstraints};
use super::resonance_matcher::ResonanceMatcher;
use super::rollout::PatchCandidate;

#[derive(Debug, Default)]
pub struct RecallOptimizer;

pub struct RecallRequest<'a> {
    pub store: &'a EpisodeMemoryStore,
    pub matcher: &'a ResonanceMatcher,
    pub bridge: &'a MemoryBridge,
    pub ctx: &'a CognitiveContext,
    pub constraints: &'a PlanningConstraints,
    pub policy: &'a AdaptivePolicy,
    pub candidates: &'a [PatchCandidate],
}

impl RecallOptimizer {
    pub fn recall(&self, request: RecallRequest<'_>) -> anyhow::Result<RecallResult> {
        let RecallRequest {
            store,
            matcher,
            bridge,
            ctx,
            constraints,
            policy,
            candidates,
        } = request;
        let workspace_root = bridge.workspace_root(ctx);
        let episodes = store.load(&workspace_root)?;
        if episodes.is_empty() {
            return Ok(RecallResult::default());
        }

        let mut best: Option<(EpisodeRecord, f32)> = None;
        for episode in episodes {
            let decayed = self.decayed_confidence(bridge, &episode, ctx, policy);
            if decayed <= 0.0 {
                continue;
            }
            let candidate_score = candidates
                .iter()
                .map(|candidate| matcher.score(bridge, ctx, candidate, &episode, decayed))
                .fold(0.0_f32, f32::max);
            if candidate_score > best.as_ref().map(|(_, score)| *score).unwrap_or(0.0) {
                best = Some((episode, candidate_score));
            }
        }

        let Some((episode, resonance_score)) = best else {
            return Ok(RecallResult::default());
        };

        let stale = self.is_stale(bridge, &episode, ctx);
        let forced_full_rollout = constraints.protected_branch
            || self.requires_full_rollout(candidates, bridge, &episode, ctx);
        let recommended_depth = if stale {
            3
        } else if resonance_score >= policy.resonance_threshold {
            1
        } else {
            3
        };
        let can_skip_rollout = !stale
            && !forced_full_rollout
            && resonance_score > policy.skip_threshold
            && episode.rollback_free_history >= 3
            && episode.protected_safe;

        Ok(RecallResult {
            matched_episode: Some(episode),
            resonance_score,
            recommended_depth,
            can_skip_rollout,
        })
    }

    pub fn decayed_confidence(
        &self,
        bridge: &MemoryBridge,
        episode: &EpisodeRecord,
        ctx: &CognitiveContext,
        policy: &AdaptivePolicy,
    ) -> f32 {
        let age = bridge.now_secs().saturating_sub(episode.created_at_secs) as f32;
        let age_days = age / 86_400.0;
        let base = (-policy.decay_lambda * age_days).exp();
        if self.is_stale(bridge, episode, ctx) {
            base * 0.35
        } else {
            base
        }
    }

    pub fn is_stale(
        &self,
        bridge: &MemoryBridge,
        episode: &EpisodeRecord,
        ctx: &CognitiveContext,
    ) -> bool {
        bridge.rollback_lineage_changed(ctx, &episode.replay_trace_hash)
            || ctx
                .dependency_graph
                .as_ref()
                .map(|graph| !graph.nodes.is_empty())
                .unwrap_or(false)
                && bridge.request_fingerprint(ctx) != episode.request_fingerprint
    }

    pub fn requires_full_rollout(
        &self,
        candidates: &[PatchCandidate],
        bridge: &MemoryBridge,
        episode: &EpisodeRecord,
        ctx: &CognitiveContext,
    ) -> bool {
        candidates
            .iter()
            .any(|candidate| candidate.estimated_files.len() > 1)
            || bridge.rollback_lineage_changed(ctx, &episode.replay_trace_hash)
    }
}
