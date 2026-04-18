use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, anyhow, bail};

use super::adaptive_policy::AdaptivePolicy;
use crate::nl::types::{CodingOptions, PlannedStep};

use super::episode_memory::EpisodeMemoryStore;
use super::episode_schema::{EpisodeRecord, RecallResult};
use super::memory_bridge::MemoryBridge;
use super::planner::{CognitiveContext, PlanResult, PlanningConstraints, ReasoningPlanner};
use super::recall_optimizer::RecallOptimizer;
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

impl MLAALPlanner {
    pub fn new(
        rollout: Arc<RolloutEngine>,
        scorer: Arc<PatchScorer>,
        replay: Arc<ReplayRolloutAdapter>,
        memory: Arc<EpisodeMemoryStore>,
        recall: Arc<RecallOptimizer>,
        resonance: Arc<ResonanceMatcher>,
        bridge: Arc<MemoryBridge>,
        telemetry: Arc<TelemetryStore>,
        threshold_optimizer: Arc<ThresholdOptimizer>,
    ) -> Self {
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
        Self::new(
            Arc::new(RolloutEngine::default()),
            Arc::new(PatchScorer),
            Arc::new(ReplayRolloutAdapter),
            Arc::new(EpisodeMemoryStore),
            Arc::new(RecallOptimizer),
            Arc::new(ResonanceMatcher),
            Arc::new(MemoryBridge),
            Arc::new(TelemetryStore),
            Arc::new(ThresholdOptimizer),
        )
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
        let request_options = CodingOptions {
            request: Some(input.to_string()),
            ..CodingOptions::default()
        };

        if input == "coding --apply" {
            return Ok(vec![PlannedPatchCandidate::single(
                PlannedStep::ApplyPreviousCodingStep,
                DiffPreview {
                    summary: "apply pending preview transaction".to_string(),
                    patch_count: 1,
                    unsafe_mutation: false,
                },
                vec![coding_target],
            )]);
        }

        if lower == "rollback" {
            return Ok(vec![PlannedPatchCandidate::single(
                PlannedStep::RollbackCurrentTransaction,
                DiffPreview {
                    summary: "rollback active transaction".to_string(),
                    patch_count: 0,
                    unsafe_mutation: false,
                },
                vec![workspace_target],
            )]);
        }

        let dependency_intent = [
            "dependency",
            "依存",
            "cycle",
            "循環",
            "interface",
            "trait",
            "extract",
            "adapter",
        ]
        .iter()
        .any(|keyword| lower.contains(keyword));
        let analyze_intent = ["analyze", "解析", "review", "調査"]
            .iter()
            .any(|keyword| lower.contains(keyword));
        let validate_intent = ["validate", "test", "確認", "検証"]
            .iter()
            .any(|keyword| lower.contains(keyword));
        let coding_intent = dependency_intent
            || [
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

        if dependency_intent || (analyze_intent && coding_intent) {
            candidates.push(PlannedPatchCandidate::new(
                vec![
                    PlannedStep::Analyze(workspace_target.clone()),
                    PlannedStep::Coding(coding_target.clone(), request_options.clone()),
                    PlannedStep::Validate(workspace_target.clone()),
                ],
                DiffPreview {
                    summary: "dependency-fix full rollout".to_string(),
                    patch_count: 1,
                    unsafe_mutation: false,
                },
                vec![coding_target.clone()],
            ));
        }

        if coding_intent && !dependency_intent {
            candidates.push(PlannedPatchCandidate::single(
                PlannedStep::Coding(coding_target.clone(), request_options.clone()),
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
                    PlannedStep::Coding(coding_target.clone(), request_options.clone()),
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

        if validate_intent {
            candidates.push(PlannedPatchCandidate::single(
                PlannedStep::Validate(workspace_target.clone()),
                DiffPreview {
                    summary: "validation-only path".to_string(),
                    patch_count: 0,
                    unsafe_mutation: false,
                },
                vec![workspace_target],
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
        let patch_candidates = candidates
            .iter()
            .map(|candidate| candidate.patch.clone())
            .collect::<Vec<_>>();
        let recall = self
            .recall
            .recall(
                &self.memory,
                &self.resonance,
                &self.bridge,
                ctx,
                constraints,
                &policy,
                &patch_candidates,
            )
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
            debug_assert!(start.elapsed().as_millis() < 1_500);
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
        debug_assert!(start.elapsed().as_millis() < 1_500);

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

fn workspace_target(target: &PathBuf) -> PathBuf {
    match target.extension().and_then(|ext| ext.to_str()) {
        Some("rs" | "toml" | "md" | "json" | "lock" | "yaml" | "yml") => PathBuf::from("."),
        _ => target.clone(),
    }
}

fn coding_target(target: &PathBuf) -> PathBuf {
    if target.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        target.clone()
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
        assert_eq!(result.planned_steps.len(), 3);
    }

    #[test]
    fn episode_saved_after_safe_success() {
        let temp = tempfile::tempdir().expect("tempdir");
        let planner = MLAALPlanner::default();
        let ctx = CognitiveContext {
            target: PathBuf::from(temp.path().join("apps/cli/src/nl/planner_v2.rs")),
            user_request: "trait を抽出して dependency cycle を解消して".to_string(),
            ir_checkpoint: Some(crate::ir::LoadedCheckpoint {
                step_index: 0,
                state: crate::service::dto::IRState {
                    workspace_root: temp.path().to_path_buf(),
                    ..crate::service::dto::IRState::default()
                },
            }),
            ..CognitiveContext::default()
        };

        planner.plan(&ctx, &constraints()).expect("plan");

        let store = EpisodeMemoryStore;
        let episodes = store.load(temp.path()).expect("episodes");
        assert_eq!(episodes.len(), 1);
        assert!(episodes[0].rollback_free);
    }

    #[test]
    fn recall_reduces_rollout_depth() {
        let temp = tempfile::tempdir().expect("tempdir");
        let store = EpisodeMemoryStore;
        let bridge = MemoryBridge;
        let recall = RecallOptimizer;
        let matcher = ResonanceMatcher;
        let ctx = CognitiveContext {
            target: PathBuf::from("apps/cli/src/nl/planner_v2.rs"),
            user_request: "trait を抽出して dependency cycle を解消して".to_string(),
            ir_checkpoint: Some(crate::ir::LoadedCheckpoint {
                step_index: 0,
                state: crate::service::dto::IRState {
                    workspace_root: temp.path().to_path_buf(),
                    ..crate::service::dto::IRState::default()
                },
            }),
            ..CognitiveContext::default()
        };
        store
            .append_episode(
                temp.path(),
                EpisodeRecord {
                    request_fingerprint: bridge.request_fingerprint(&ctx),
                    dependency_signature: bridge.dependency_signature(
                        &ctx,
                        &PatchCandidate {
                            step: PlannedStep::Analyze(PathBuf::from(".")),
                            diff_preview: DiffPreview {
                                summary: "dependency-fix full rollout".to_string(),
                                patch_count: 1,
                                unsafe_mutation: false,
                            },
                            estimated_files: vec![PathBuf::from("apps/cli/src/nl/planner_v2.rs")],
                        },
                    ),
                    rollout_path: vec![
                        PlannedStep::Analyze(PathBuf::from(".")),
                        PlannedStep::Coding(
                            PathBuf::from("apps/cli/src/nl/planner_v2.rs"),
                            CodingOptions {
                                request: Some(ctx.user_request.clone()),
                                ..CodingOptions::default()
                            },
                        ),
                        PlannedStep::Validate(PathBuf::from(".")),
                    ],
                    final_score: 0.91,
                    rollback_free: true,
                    protected_safe: true,
                    replay_trace_hash: bridge.replay_trace_hash(ctx.replay_timeline.as_ref()),
                    created_at_secs: bridge.now_secs(),
                    rollback_free_history: 3,
                },
            )
            .expect("append episode");
        let candidates = vec![PatchCandidate {
            step: PlannedStep::Analyze(PathBuf::from(".")),
            diff_preview: DiffPreview {
                summary: "dependency-fix full rollout".to_string(),
                patch_count: 1,
                unsafe_mutation: false,
            },
            estimated_files: vec![PathBuf::from("apps/cli/src/nl/planner_v2.rs")],
        }];

        let result = recall
            .recall(
                &store,
                &matcher,
                &bridge,
                &ctx,
                &constraints(),
                &AdaptivePolicy::default(),
                &candidates,
            )
            .expect("recall");
        assert_eq!(result.recommended_depth, 1);
    }

    #[test]
    fn high_resonance_skips_rollout() {
        let temp = tempfile::tempdir().expect("tempdir");
        let planner = MLAALPlanner::default();
        let bridge = MemoryBridge;
        let store = EpisodeMemoryStore;
        let request = "trait を抽出して dependency cycle を解消して".to_string();
        let ctx = CognitiveContext {
            target: PathBuf::from("apps/cli/src/nl/planner_v2.rs"),
            user_request: request.clone(),
            ir_checkpoint: Some(crate::ir::LoadedCheckpoint {
                step_index: 0,
                state: crate::service::dto::IRState {
                    workspace_root: temp.path().to_path_buf(),
                    ..crate::service::dto::IRState::default()
                },
            }),
            ..CognitiveContext::default()
        };
        store
            .append_episode(
                temp.path(),
                EpisodeRecord {
                    request_fingerprint: bridge.request_fingerprint(&ctx),
                    dependency_signature: bridge.dependency_signature(
                        &ctx,
                        &PatchCandidate {
                            step: PlannedStep::Analyze(PathBuf::from(".")),
                            diff_preview: DiffPreview {
                                summary: "dependency-fix full rollout".to_string(),
                                patch_count: 1,
                                unsafe_mutation: false,
                            },
                            estimated_files: vec![PathBuf::from("apps/cli/src/nl/planner_v2.rs")],
                        },
                    ),
                    rollout_path: vec![
                        PlannedStep::Analyze(PathBuf::from(".")),
                        PlannedStep::Coding(
                            PathBuf::from("apps/cli/src/nl/planner_v2.rs"),
                            CodingOptions {
                                request: Some(request),
                                ..CodingOptions::default()
                            },
                        ),
                        PlannedStep::Validate(PathBuf::from(".")),
                    ],
                    final_score: 0.95,
                    rollback_free: true,
                    protected_safe: true,
                    replay_trace_hash: bridge.replay_trace_hash(ctx.replay_timeline.as_ref()),
                    created_at_secs: bridge.now_secs(),
                    rollback_free_history: 4,
                },
            )
            .expect("append episode");

        let result = planner.plan(&ctx, &constraints()).expect("plan");
        assert_eq!(result.planned_steps.len(), 3);
        assert!(result.confidence > 0.92);
    }

    #[test]
    fn protected_branch_forces_full_rollout() {
        let planner = MLAALPlanner::new(
            Arc::new(RolloutEngine::default()),
            Arc::new(PatchScorer),
            Arc::new(ReplayRolloutAdapter),
            Arc::new(EpisodeMemoryStore),
            Arc::new(RecallOptimizer),
            Arc::new(ResonanceMatcher),
            Arc::new(MemoryBridge),
            Arc::new(TelemetryStore),
            Arc::new(ThresholdOptimizer),
        );
        let ctx = CognitiveContext {
            target: PathBuf::from("apps/cli/src/nl/planner_v2.rs"),
            user_request: "trait を抽出して dependency cycle を解消して".to_string(),
            ..CognitiveContext::default()
        };
        let mut constraints = constraints();
        constraints.protected_branch = true;

        let result = planner.plan(&ctx, &constraints).expect("plan");
        assert_eq!(result.planned_steps.len(), 3);
    }

    #[test]
    fn stale_episode_triggers_decay() {
        let bridge = MemoryBridge;
        let recall = RecallOptimizer;
        let old_episode = EpisodeRecord {
            request_fingerprint: "old".to_string(),
            dependency_signature: "sig".to_string(),
            rollout_path: vec![PlannedStep::Analyze(PathBuf::from("."))],
            final_score: 0.81,
            rollback_free: true,
            protected_safe: true,
            replay_trace_hash: "old-trace".to_string(),
            created_at_secs: bridge.now_secs().saturating_sub(60 * 60 * 24 * 60),
            rollback_free_history: 3,
        };
        let ctx = CognitiveContext {
            target: PathBuf::from("apps/cli/src/nl/planner_v2.rs"),
            user_request: "new request".to_string(),
            replay_timeline: Some(vec![crate::ir::ReplayTimelineEntry {
                step: 1,
                action: "Analyze".to_string(),
                target: Some(".".to_string()),
                state_hash: "new-trace".to_string(),
                next_actions: vec!["Analyze".to_string()],
            }]),
            ..CognitiveContext::default()
        };

        let decayed =
            recall.decayed_confidence(&bridge, &old_episode, &ctx, &AdaptivePolicy::default());
        assert!(decayed < 0.35);
    }

    #[test]
    fn telemetry_record_written() {
        let temp = tempfile::tempdir().expect("tempdir");
        let planner = MLAALPlanner::default();
        let ctx = CognitiveContext {
            target: PathBuf::from(temp.path().join("apps/cli/src/nl/planner_v2.rs")),
            user_request: "trait を抽出して dependency cycle を解消して".to_string(),
            ir_checkpoint: Some(crate::ir::LoadedCheckpoint {
                step_index: 0,
                state: crate::service::dto::IRState {
                    workspace_root: temp.path().to_path_buf(),
                    ..crate::service::dto::IRState::default()
                },
            }),
            ..CognitiveContext::default()
        };

        planner.plan(&ctx, &constraints()).expect("plan");

        let telemetry = TelemetryStore;
        let records = telemetry.load(temp.path()).expect("telemetry");
        assert_eq!(records.len(), 1);
    }

    #[test]
    fn recall_hit_rate_adjusts_threshold() {
        let optimizer = ThresholdOptimizer;
        let current = AdaptivePolicy::default();
        let kpi = crate::mlaal::TelemetryWindowKpi {
            recall_hit_rate: 0.10,
            ..crate::mlaal::TelemetryWindowKpi::default()
        };

        let next = optimizer.optimize(&current, &kpi);
        assert!(next.resonance_threshold < current.resonance_threshold);
    }

    #[test]
    fn unsafe_skip_increases_threshold() {
        let optimizer = ThresholdOptimizer;
        let current = AdaptivePolicy::default();
        let kpi = crate::mlaal::TelemetryWindowKpi {
            rollout_skip_rate: 0.30,
            safe_reuse_success_rate: 0.80,
            ..crate::mlaal::TelemetryWindowKpi::default()
        };

        let next = optimizer.optimize(&current, &kpi);
        assert!(next.skip_threshold > current.skip_threshold);
    }

    #[test]
    fn high_safe_reuse_reduces_depth() {
        let policy = AdaptivePolicy {
            depth_shrink_ratio: 0.50,
            beam_shrink_ratio: 0.50,
            ..AdaptivePolicy::default()
        };
        let planner = MLAALPlanner::default();
        let recall = RecallResult {
            matched_episode: Some(EpisodeRecord {
                request_fingerprint: "fp".to_string(),
                dependency_signature: "dep".to_string(),
                rollout_path: vec![PlannedStep::Analyze(PathBuf::from("."))],
                final_score: 0.9,
                rollback_free: true,
                protected_safe: true,
                replay_trace_hash: "trace".to_string(),
                created_at_secs: 0,
                rollback_free_history: 3,
            }),
            resonance_score: 0.95,
            recommended_depth: 1,
            can_skip_rollout: false,
        };

        let adjusted = planner.adjusted_rollout_engine(&recall, &policy);
        assert!(adjusted.max_depth < planner.rollout.max_depth);
    }

    #[test]
    fn latency_budget_improves_over_window() {
        let telemetry = TelemetryStore;
        let records = vec![
            TelemetryRecord {
                recall_hit: true,
                rollout_skipped: true,
                rollout_depth: 0,
                beam_width: 0,
                preview_latency_ms: 700,
                rollback_free: true,
                protected_safe: true,
                resonance_score: 0.95,
                decay_applied: false,
                replay_divergence: 0.0,
            },
            TelemetryRecord {
                recall_hit: false,
                rollout_skipped: false,
                rollout_depth: 3,
                beam_width: 4,
                preview_latency_ms: 1300,
                rollback_free: true,
                protected_safe: true,
                resonance_score: 0.30,
                decay_applied: false,
                replay_divergence: 0.2,
            },
        ];
        let kpi = telemetry.compute_kpi(&records);
        assert!(kpi.avg_preview_latency_ms < 1_100.0);
    }
}
