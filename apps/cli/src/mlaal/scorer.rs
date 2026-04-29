use crate::nl::types::PlannedStep;

use super::planner::{CognitiveContext, PlanningConstraints};
use super::rollout::{PatchCandidate, RolloutState};

const ALPHA: f32 = 0.35;
const BETA: f32 = 0.35;
const GAMMA: f32 = 0.20;
const DELTA: f32 = 0.10;
const BRANCH_SAFETY_THRESHOLD: f32 = 0.25;
const REPLAY_DIVERGENCE_HARD_LIMIT: f32 = 0.85;

#[derive(Debug, Clone, PartialEq)]
pub struct ScoreVector {
    pub total: f32,
    pub goal_fit: f32,
    pub rollback_risk: f32,
    pub replay_divergence: f32,
    pub branch_safety: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CandidateScore {
    pub vector: ScoreVector,
    pub rejected: bool,
    pub reject_reason: Option<&'static str>,
}

#[derive(Debug, Default)]
pub struct PatchScorer;

impl PatchScorer {
    pub fn score(
        &self,
        ctx: &CognitiveContext,
        constraints: &PlanningConstraints,
        candidate: &PatchCandidate,
        rollout: &RolloutState,
    ) -> CandidateScore {
        let goal_fit = goal_fit(ctx, candidate);
        let rollback_risk = rollback_risk(constraints, candidate, rollout);
        let replay_divergence = rollout.divergence_score.clamp(0.0, 1.0);
        let branch_safety = branch_safety(constraints, candidate, rollout);
        let total = (ALPHA * goal_fit) - (BETA * rollback_risk) - (GAMMA * replay_divergence)
            + (DELTA * branch_safety);

        let vector = ScoreVector {
            total,
            goal_fit,
            rollback_risk,
            replay_divergence,
            branch_safety,
        };

        let (rejected, reject_reason) =
            reject_reason(&vector, rollout).map_or((false, None), |reason| (true, Some(reason)));

        CandidateScore {
            vector,
            rejected,
            reject_reason,
        }
    }
}

fn reject_reason(score: &ScoreVector, rollout: &RolloutState) -> Option<&'static str> {
    if !rollout.rollback_available {
        return Some("rollback_unavailable");
    }
    if score.branch_safety < BRANCH_SAFETY_THRESHOLD {
        return Some("branch_safety_below_threshold");
    }
    if score.replay_divergence > REPLAY_DIVERGENCE_HARD_LIMIT {
        return Some("replay_divergence_above_limit");
    }
    None
}

fn goal_fit(ctx: &CognitiveContext, candidate: &PatchCandidate) -> f32 {
    let lower = ctx.user_request.to_lowercase();
    let dependency_fix_intent = [
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

    let preview_safety_intent = ["safe", "preview", "安全", "rollback"]
        .iter()
        .any(|keyword| lower.contains(keyword));

    let base: f32 = match candidate.step {
        PlannedStep::Refactor(_) => 0.78,
        PlannedStep::Analyze(_) => 0.55,
        PlannedStep::Repair(_) => 0.65,
        PlannedStep::Apply => 0.70,
        PlannedStep::Reload => 0.68,
        _ => 0.50,
    };

    let dependency_bonus: f32 = if dependency_fix_intent {
        match candidate.step {
            PlannedStep::Refactor(_) => 0.17,
            PlannedStep::Analyze(_) | PlannedStep::Repair(_) => 0.08,
            _ => 0.03,
        }
    } else {
        0.0
    };

    let preview_bonus: f32 = if preview_safety_intent && candidate.diff_preview.unsafe_mutation {
        -0.10
    } else if preview_safety_intent {
        0.08
    } else {
        0.0
    };

    let targeting_bonus: f32 = if candidate
        .estimated_files
        .iter()
        .any(|path| path == &ctx.target)
    {
        0.05
    } else if candidate.estimated_files.len() <= 1 {
        0.03
    } else {
        0.0
    };

    (base + dependency_bonus + preview_bonus + targeting_bonus).clamp(0.0_f32, 1.0_f32)
}

fn rollback_risk(
    constraints: &PlanningConstraints,
    candidate: &PatchCandidate,
    rollout: &RolloutState,
) -> f32 {
    let mut risk = if rollout.rollback_available {
        0.10
    } else {
        1.0
    };

    risk += (candidate.estimated_files.len() as f32 * 0.12).min(0.48);
    risk += candidate.diff_preview.patch_count as f32 * 0.05;

    if candidate.diff_preview.unsafe_mutation {
        risk += 0.20;
    }
    if constraints.protected_branch && is_write_step(&candidate.step) {
        risk += 0.35;
    }
    if matches!(candidate.step, PlannedStep::Apply) {
        risk += 0.10;
    }

    risk.clamp(0.0_f32, 1.0_f32)
}

fn branch_safety(
    constraints: &PlanningConstraints,
    candidate: &PatchCandidate,
    rollout: &RolloutState,
) -> f32 {
    if constraints.protected_branch && is_write_step(&candidate.step) {
        return 0.0;
    }

    let mut safety: f32 = if constraints.protected_branch {
        0.55
    } else {
        0.95
    };
    if candidate.diff_preview.unsafe_mutation {
        safety -= 0.25;
    }
    if candidate.estimated_files.len() > 2 {
        safety -= 0.10;
    }
    if !rollout.rollback_available {
        safety -= 0.30;
    }
    safety.clamp(0.0_f32, 1.0_f32)
}

fn is_write_step(step: &PlannedStep) -> bool {
    matches!(
        step,
        PlannedStep::Refactor(_) | PlannedStep::Apply | PlannedStep::Repair(_)
    )
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::nl::types::{PlannedStep, RefactorSpec};

    use super::*;
    use crate::mlaal::rollout::{DiffPreview, PatchCandidate, RolloutState};

    fn ctx(request: &str) -> CognitiveContext {
        CognitiveContext {
            target: PathBuf::from("apps/cli/src/nl/planner_v2.rs"),
            user_request: request.to_string(),
            ..CognitiveContext::default()
        }
    }

    fn constraints() -> PlanningConstraints {
        PlanningConstraints {
            preview_required: true,
            rollback_safe: true,
            protected_branch: false,
            max_rollout_depth: 3,
        }
    }

    #[test]
    fn protected_branch_rejects_unsafe_candidate() {
        let scorer = PatchScorer;
        let candidate = PatchCandidate {
            step: PlannedStep::Refactor(RefactorSpec {
                target: PathBuf::from("apps/cli/src/nl/planner_v2.rs"),
                request: "request".to_string(),
            }),
            diff_preview: DiffPreview {
                summary: "unsafe multi-file patch".to_string(),
                patch_count: 2,
                unsafe_mutation: true,
            },
            estimated_files: vec![
                PathBuf::from("apps/cli/src/nl/planner_v2.rs"),
                PathBuf::from("apps/cli/src/nl/mod.rs"),
            ],
        };
        let rollout = RolloutState {
            depth: 2,
            predicted_ir: None,
            divergence_score: 0.20,
            rollback_available: true,
        };
        let mut constraints = constraints();
        constraints.protected_branch = true;

        let score = scorer.score(&ctx("trait 抽出して"), &constraints, &candidate, &rollout);

        assert!(score.rejected);
        assert_eq!(score.reject_reason, Some("branch_safety_below_threshold"));
    }
}
