use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, anyhow, bail};

use super::adaptive_policy::AdaptivePolicy;
use crate::nl::types::{PlannedStep, RefactorSpec};

use super::episode_memory::EpisodeMemoryStore;
use super::episode_schema::{EpisodeRecord, RecallResult};
use super::memory_bridge::MemoryBridge;
use super::planner::{CognitiveContext, PlanResult, PlanningConstraints, ReasoningPlanner};
use super::recall_optimizer::{RecallOptimizer, RecallRequest};
use super::replay_rollout::ReplayRolloutAdapter;
use super::resonance_matcher::ResonanceMatcher;
use super::rollout::{CandidateRollout, DiffPreview, PatchCandidate, RolloutEngine};
use super::scorer::{CandidateScore, PatchScorer};
use super::telemetry::TelemetryStore;
use super::telemetry_schema::TelemetryRecord;
use super::threshold_optimizer::ThresholdOptimizer;

pub struct MLAALPlanner {
    rollout: Arc<RolloutEngine>,
    scorer: Arc<PatchScorer>,
    replay: Arc<ReplayRolloutAdapter>,
    memory: Arc<EpisodeMemoryStore>,
    recall: Arc<RecallOptimizer>,
    resonance: Arc<ResonanceMatcher>,
    bridge: Arc<MemoryBridge>,
    telemetry: Arc<TelemetryStore>,
    threshold_optimizer: Arc<ThresholdOptimizer>,
}

pub struct MLAALPlannerComponents {
    rollout: Arc<RolloutEngine>,
    scorer: Arc<PatchScorer>,
    replay: Arc<ReplayRolloutAdapter>,
    memory: Arc<EpisodeMemoryStore>,
    recall: Arc<RecallOptimizer>,
    resonance: Arc<ResonanceMatcher>,
    bridge: Arc<MemoryBridge>,
    telemetry: Arc<TelemetryStore>,
    threshold_optimizer: Arc<ThresholdOptimizer>,
}

impl MLAALPlanner {
    pub fn new(components: MLAALPlannerComponents) -> Self {
        let MLAALPlannerComponents {
            rollout,
            scorer,
            replay,
            memory,
            recall,
            resonance,
            bridge,
            telemetry,
            threshold_optimizer,
        } = components;
        Self {
            rollout,
            scorer,
            replay,
            memory,
            recall,
            resonance,
            bridge,
            telemetry,
            threshold_optimizer,
        }
    }

    pub fn default_stack() -> Self {
        Self::new(MLAALPlannerComponents {
            rollout: Arc::new(RolloutEngine::default()),
            scorer: Arc::new(PatchScorer),
            replay: Arc::new(ReplayRolloutAdapter),
            memory: Arc::new(EpisodeMemoryStore),
            recall: Arc::new(RecallOptimizer),
            resonance: Arc::new(ResonanceMatcher),
            bridge: Arc::new(MemoryBridge),
            telemetry: Arc::new(TelemetryStore),
            threshold_optimizer: Arc::new(ThresholdOptimizer),
        })
    }

    fn generate_candidates(
        &self,
        ctx: &CognitiveContext,
    ) -> anyhow::Result<Vec<PlannedPatchCandidate>> {
        let input = ctx.user_request.trim();
        if input.is_empty() {
            bail!("planner unavailable: empty request")
        }

        if input.eq_ignore_ascii_case("force legacy fallback") {
            bail!("planner unavailable: explicit fallback requested")
        }

        let lower = input.to_lowercase();
        let workspace_target = workspace_target(&ctx.target);
        let coding_target = coding_target(&ctx.target);

        if input == "coding --apply" || input == "apply" || input == "適用" {
            return Ok(vec![PlannedPatchCandidate::single(
                PlannedStep::Apply,
                DiffPreview {
                    summary: "apply pending preview transaction".to_string(),
                    patch_count: 1,
                    unsafe_mutation: false,
                },
                vec![coding_target],
            )]);
        }

        if lower == "rollback" || lower == "再同期" || lower == "reload" {
            return Ok(vec![PlannedPatchCandidate::single(
                PlannedStep::Reload,
                DiffPreview {
                    summary: "reload IR from disk".to_string(),
                    patch_count: 0,
                    unsafe_mutation: false,
                },
                vec![workspace_target],
            )]);
        }

        let analyze_intent = [
            "analyze",
            "解析",
            "review",
            "調査",
            "dependency",
            "依存",
            "cycle",
            "循環",
        ]
        .iter()
        .any(|keyword| lower.contains(keyword));
        let coding_intent = [
            "fix",
            "修正",
            "変更",
            "改善",
            "追加",
            "refactor",
            "直して",
            "trait",
            "adapter",
        ]
        .iter()
        .any(|keyword| lower.contains(keyword));

        let mut candidates = Vec::new();

        if coding_intent {
            let refactor_spec = RefactorSpec {
                target: coding_target.clone(),
                request: input.to_string(),
            };
            candidates.push(PlannedPatchCandidate::single(
                PlannedStep::Refactor(refactor_spec.clone()),
                DiffPreview {
                    summary: "targeted coding patch".to_string(),
                    patch_count: 1,
                    unsafe_mutation: false,
                },
                vec![coding_target.clone()],
            ));
            candidates.push(PlannedPatchCandidate::new(
                vec![
                    PlannedStep::Analyze(workspace_target.clone()),
                    PlannedStep::Refactor(refactor_spec),
                ],
                DiffPreview {
                    summary: "analyze then patch".to_string(),
                    patch_count: 1,
                    unsafe_mutation: false,
                },
                vec![coding_target.clone()],
            ));
        }

        if analyze_intent || candidates.is_empty() {
            candidates.push(PlannedPatchCandidate::single(
                PlannedStep::Analyze(workspace_target.clone()),
                DiffPreview {
                    summary: "analysis-only path".to_string(),
                    patch_count: 0,
                    unsafe_mutation: false,
                },
                vec![workspace_target.clone()],
            ));
        }

        Ok(candidates)
    }
}

impl Default for MLAALPlanner {
    fn default() -> Self {
        Self::default_stack()
    }
}

impl ReasoningPlanner for MLAALPlanner {
    fn plan(
        &self,
        ctx: &CognitiveContext,
        constraints: &PlanningConstraints,
    ) -> anyhow::Result<PlanResult> {
        let start = Instant::now();
        let workspace_root = self.bridge.workspace_root(ctx);
        let policy = AdaptivePolicy::load(&workspace_root).unwrap_or_default();
        let candidates = self.generate_candidates(ctx)?;

        // R3: "apply" / "reload" shortcuts bypass cognitive rollout
        if candidates.len() == 1 {
            let first = &candidates[0];
            if matches!(
                first.planned_steps[0],
                PlannedStep::Apply | PlannedStep::Reload
            ) {
                return Ok(PlanResult {
                    selected_action: first.planned_steps[0].clone(),
                    confidence: 1.0,
                    risk_score: 0.0,
                    compatibility_mode: true,
                    planned_steps: first.planned_steps.clone(),
                });
            }
        }

        let patch_candidates = candidates
            .iter()
            .map(|candidate| candidate.patch.clone())
            .collect::<Vec<_>>();
        let recall = self
            .recall
            .recall(RecallRequest {
                store: &self.memory,
                matcher: &self.resonance,
                bridge: &self.bridge,
                ctx,
                constraints,
                policy: &policy,
                candidates: &patch_candidates,
            })
            .context("episode recall failed")?;

        if let Some(result) = self.try_recall_first(ctx, constraints, &candidates, &recall)? {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            self.record_telemetry_and_optimize(
                &workspace_root,
                TelemetryRecord {
                    recall_hit: recall.matched_episode.is_some(),
                    rollout_skipped: true,
                    rollout_depth: 0,
                    beam_width: 0,
                    preview_latency_ms: elapsed_ms,
                    rollback_free: true,
                    protected_safe: !constraints.protected_branch,
                    resonance_score: recall.resonance_score,
                    decay_applied: self.decay_applied(ctx, &policy, &recall),
                    replay_divergence: 0.0,
                },
                &policy,
            )?;
            return Ok(result);
        }

        let rollout_engine = self.adjusted_rollout_engine(&recall, &policy);
        let rollout_candidates =
            rollout_engine.evaluate_candidates(ctx, constraints, &self.replay, patch_candidates);

        let (best, score, selected_rollout) =
            self.select_best_rollout(ctx, constraints, &candidates, &rollout_candidates)?;
        self.persist_episode_if_safe(ctx, constraints, &best, &score, &selected_rollout.state)?;
        let elapsed_ms = start.elapsed().as_millis() as u64;
        self.record_telemetry_and_optimize(
            &workspace_root,
            TelemetryRecord {
                recall_hit: recall.matched_episode.is_some(),
                rollout_skipped: false,
                rollout_depth: selected_rollout.state.depth,
                beam_width: rollout_engine.beam_width,
                preview_latency_ms: elapsed_ms,
                rollback_free: selected_rollout.state.rollback_available
                    && score.vector.rollback_risk < 0.35,
                protected_safe: !constraints.protected_branch,
                resonance_score: recall.resonance_score,
                decay_applied: self.decay_applied(ctx, &policy, &recall),
                replay_divergence: selected_rollout.state.divergence_score,
            },
            &policy,
        )?;

        Ok(PlanResult {
            selected_action: best
                .planned_steps
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("selected rollout candidate returned empty plan"))?,
            confidence: (1.0 - score.vector.replay_divergence).clamp(0.0, 1.0),
            risk_score: score.vector.rollback_risk,
            compatibility_mode: false,
            planned_steps: best.planned_steps,
        })
    }
}

impl MLAALPlanner {
    fn try_recall_first(
        &self,
        ctx: &CognitiveContext,
        constraints: &PlanningConstraints,
        candidates: &[PlannedPatchCandidate],
        recall: &RecallResult,
    ) -> anyhow::Result<Option<PlanResult>> {
        if !recall.can_skip_rollout {
            return Ok(None);
        }
        let Some(episode) = recall.matched_episode.as_ref() else {
            return Ok(None);
        };
        let Some(candidate) = candidates
            .iter()
            .find(|candidate| candidate.planned_steps == episode.rollout_path)
            .cloned()
        else {
            return Ok(None);
        };
        let rollout_state = super::rollout::RolloutState {
            depth: 0,
            predicted_ir: ctx.ir_checkpoint.clone(),
            divergence_score: 0.0,
            rollback_available: true,
        };
        let score = self
            .scorer
            .score(ctx, constraints, &candidate.patch, &rollout_state);
        if score.rejected {
            return Ok(None);
        }
        self.persist_episode_if_safe(ctx, constraints, &candidate, &score, &rollout_state)?;
        Ok(Some(PlanResult {
            selected_action: candidate
                .planned_steps
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("recall-selected candidate returned empty plan"))?,
            confidence: recall.resonance_score.clamp(0.0, 1.0),
            risk_score: score.vector.rollback_risk,
            compatibility_mode: false,
            planned_steps: candidate.planned_steps,
        }))
    }

    fn adjusted_rollout_engine(
        &self,
        recall: &RecallResult,
        policy: &AdaptivePolicy,
    ) -> RolloutEngine {
        if recall.matched_episode.is_some() && recall.recommended_depth == 1 {
            RolloutEngine {
                max_depth: shrink_depth(self.rollout.max_depth, policy.depth_shrink_ratio),
                beam_width: shrink_beam(self.rollout.beam_width, policy.beam_shrink_ratio),
            }
        } else {
            (*self.rollout).clone()
        }
    }

    fn select_best_rollout(
        &self,
        ctx: &CognitiveContext,
        constraints: &PlanningConstraints,
        candidates: &[PlannedPatchCandidate],
        rollout_candidates: &[CandidateRollout],
    ) -> anyhow::Result<(PlannedPatchCandidate, CandidateScore, CandidateRollout)> {
        let mut best: Option<(PlannedPatchCandidate, CandidateScore, CandidateRollout)> = None;
        for rollout in rollout_candidates.iter().cloned() {
            let candidate = candidates
                .iter()
                .find(|candidate| candidate.patch == rollout.candidate)
                .cloned()
                .ok_or_else(|| anyhow!("candidate lookup failed during rollout aggregation"))?;
            let score = self
                .scorer
                .score(ctx, constraints, &candidate.patch, &rollout.state);
            if score.rejected {
                continue;
            }
            let replace = best
                .as_ref()
                .map(|(_, current, _)| score.vector.total > current.vector.total)
                .unwrap_or(true);
            if replace {
                best = Some((candidate, score, rollout));
            }
        }
        best.context("no safe rollout candidate available")
    }

    fn persist_episode_if_safe(
        &self,
        ctx: &CognitiveContext,
        constraints: &PlanningConstraints,
        candidate: &PlannedPatchCandidate,
        score: &CandidateScore,
        rollout_state: &super::rollout::RolloutState,
    ) -> anyhow::Result<()> {
        let workspace_root = self.bridge.workspace_root(ctx);
        let rollback_free = rollout_state.rollback_available && score.vector.rollback_risk < 0.35;
        let protected_safe = !constraints.protected_branch && score.vector.branch_safety >= 0.80;
        let preview_accepted = constraints.preview_required;
        let threshold_met = score.vector.total.is_finite();
        if !(rollback_free && protected_safe && preview_accepted && threshold_met) {
            return Ok(());
        }

        let episode = EpisodeRecord {
            request_fingerprint: self.bridge.request_fingerprint(ctx),
            dependency_signature: self.bridge.dependency_signature(ctx, &candidate.patch),
            rollout_path: candidate.planned_steps.clone(),
            final_score: score.vector.total,
            rollback_free,
            protected_safe,
            replay_trace_hash: self.bridge.replay_trace_hash(ctx.replay_timeline.as_ref()),
            created_at_secs: self.bridge.now_secs(),
            rollback_free_history: 3,
        };
        self.memory.append_episode(&workspace_root, episode)
    }

    fn record_telemetry_and_optimize(
        &self,
        workspace_root: &std::path::Path,
        telemetry_record: TelemetryRecord,
        current_policy: &AdaptivePolicy,
    ) -> anyhow::Result<()> {
        self.telemetry.append(workspace_root, telemetry_record)?;
        let window = self.telemetry.latest_window(workspace_root, 100)?;
        let kpi = self.telemetry.compute_kpi(&window);
        let next = self.threshold_optimizer.optimize(current_policy, &kpi);
        next.save(workspace_root)
    }

    fn decay_applied(
        &self,
        ctx: &CognitiveContext,
        policy: &AdaptivePolicy,
        recall: &RecallResult,
    ) -> bool {
        recall
            .matched_episode
            .as_ref()
            .map(|episode| {
                self.recall
                    .decayed_confidence(&self.bridge, episode, ctx, policy)
                    < 0.999
            })
            .unwrap_or(false)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlannedPatchCandidate {
    patch: PatchCandidate,
    planned_steps: Vec<PlannedStep>,
}

impl PlannedPatchCandidate {
    fn new(
        planned_steps: Vec<PlannedStep>,
        diff_preview: DiffPreview,
        estimated_files: Vec<PathBuf>,
    ) -> Self {
        let step = planned_steps
            .first()
            .cloned()
            .unwrap_or(PlannedStep::Analyze(PathBuf::from(".")));
        Self {
            patch: PatchCandidate {
                step,
                diff_preview,
                estimated_files,
            },
            planned_steps,
        }
    }

    fn single(step: PlannedStep, diff_preview: DiffPreview, estimated_files: Vec<PathBuf>) -> Self {
        Self::new(vec![step], diff_preview, estimated_files)
    }
}

fn workspace_target(target: &Path) -> PathBuf {
    match target.extension().and_then(|ext| ext.to_str()) {
        Some("rs" | "toml" | "md" | "json" | "lock" | "yaml" | "yml") => PathBuf::from("."),
        _ => target.to_path_buf(),
    }
}

fn coding_target(target: &Path) -> PathBuf {
    if target.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        target.to_path_buf()
    }
}

fn shrink_depth(base: usize, ratio: f32) -> usize {
    let reduced = (base as f32 * ratio).round() as usize;
    reduced.clamp(1, base.max(1))
}

fn shrink_beam(base: usize, ratio: f32) -> usize {
    let reduced = (base as f32 * ratio).round() as usize;
    reduced.clamp(1, base.max(1))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::mlaal::{
        AdaptivePolicy, CognitiveContext, EpisodeMemoryStore, MemoryBridge, PlanningConstraints,
        RecallOptimizer, ResonanceMatcher, RolloutEngine, TelemetryStore, ThresholdOptimizer,
    };
    use crate::nl::types::PlannedStep;

    fn constraints() -> PlanningConstraints {
        PlanningConstraints {
            preview_required: true,
            rollback_safe: true,
            protected_branch: false,
            max_rollout_depth: 3,
        }
    }

    #[test]
    fn mlaal_planner_meets_preview_latency_budget() {
        let planner = MLAALPlanner::default();
        let ctx = CognitiveContext {
            target: PathBuf::from("apps/cli/src/nl/planner_v2.rs"),
            user_request: "trait を抽出して dependency cycle を解消して".to_string(),
            ..CognitiveContext::default()
        };

        let start = Instant::now();
        let result = planner.plan(&ctx, &constraints()).expect("plan");

        assert!(start.elapsed().as_millis() < 1_500);
        assert!(matches!(result.selected_action, PlannedStep::Analyze(_)));
    }
}
