use super::episode_schema::EpisodeRecord;
use super::memory_bridge::MemoryBridge;
use super::planner::CognitiveContext;
use super::rollout::PatchCandidate;

const ALPHA: f32 = 0.35;
const BETA: f32 = 0.25;
const GAMMA: f32 = 0.20;
const DELTA: f32 = 0.20;

#[derive(Debug, Default)]
pub struct ResonanceMatcher;

impl ResonanceMatcher {
    pub fn score(
        &self,
        bridge: &MemoryBridge,
        ctx: &CognitiveContext,
        candidate: &PatchCandidate,
        episode: &EpisodeRecord,
        decayed_confidence: f32,
    ) -> f32 {
        let request = request_similarity(bridge, ctx, episode);
        let graph = graph_similarity(bridge, ctx, candidate, episode);
        let diff = diff_similarity(bridge, candidate, episode);
        let trace = trace_similarity(bridge, ctx, episode);
        ((ALPHA * request) + (BETA * graph) + (GAMMA * diff) + (DELTA * trace)) * decayed_confidence
    }
}

fn request_similarity(
    bridge: &MemoryBridge,
    ctx: &CognitiveContext,
    episode: &EpisodeRecord,
) -> f32 {
    if bridge.request_fingerprint(ctx) == episode.request_fingerprint {
        1.0
    } else {
        let current = ctx.user_request.to_lowercase();
        let target_tokens = [
            "trait",
            "interface",
            "dependency",
            "cycle",
            "rollback",
            "preview",
        ];
        let overlap = target_tokens
            .iter()
            .filter(|token| current.contains(**token))
            .count() as f32;
        (0.35 + (overlap * 0.10)).clamp(0.0, 0.9)
    }
}

fn graph_similarity(
    bridge: &MemoryBridge,
    ctx: &CognitiveContext,
    candidate: &PatchCandidate,
    episode: &EpisodeRecord,
) -> f32 {
    if bridge.dependency_signature(ctx, candidate) == episode.dependency_signature {
        1.0
    } else if candidate.estimated_files.len() == episode.rollout_path.len() {
        0.60
    } else {
        0.35
    }
}

fn diff_similarity(
    bridge: &MemoryBridge,
    candidate: &PatchCandidate,
    episode: &EpisodeRecord,
) -> f32 {
    let candidate_pattern = bridge.rollout_path_pattern(std::slice::from_ref(&candidate.step));
    let episode_pattern = bridge.rollout_path_pattern(&episode.rollout_path);
    if candidate_pattern == episode_pattern {
        0.95
    } else if candidate
        .diff_preview
        .summary
        .to_lowercase()
        .contains("dependency")
        && episode.rollback_free
    {
        0.72
    } else {
        0.40
    }
}

fn trace_similarity(bridge: &MemoryBridge, ctx: &CognitiveContext, episode: &EpisodeRecord) -> f32 {
    if bridge.replay_trace_hash(ctx.replay_timeline.as_ref()) == episode.replay_trace_hash {
        1.0
    } else if ctx.replay_timeline.is_some() && episode.replay_trace_hash != "no-trace" {
        0.55
    } else {
        0.30
    }
}
