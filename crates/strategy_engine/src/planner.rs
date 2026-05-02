use crate::candidate::StrategyCandidate;
use crate::failure::{FailureContext, FailureKind};
use crate::history::ExecutionHistory;
use crate::types::{CodeIrProgram, ExecutionMode};
use execution_core::engine::execution_plan::*;

/// Generates repair and replan candidates from a failure context.
///
/// Spec §10 AdaptivePlanner
///
/// Capabilities (spec §10.1):
/// - IR部分修正: modify individual commands in the failing phase
/// - step挿入: prepend a corrective step to the affected phase
/// - step削除: remove the failing step
/// - parameter変更: change command-line flags
///
/// Constraints (spec §10.2):
/// - All modified plans pass structural validation before they are returned.
/// - No plan may introduce safety-violating commands (shell execution, etc.).
#[derive(Debug, Clone)]
pub struct AdaptivePlanner {
    pub allow_repair: bool,
    pub allow_replan: bool,
}

impl Default for AdaptivePlanner {
    fn default() -> Self {
        Self {
            allow_repair: true,
            allow_replan: true,
        }
    }
}

impl AdaptivePlanner {
    pub fn new(allow_repair: bool, allow_replan: bool) -> Self {
        Self {
            allow_repair,
            allow_replan,
        }
    }

    /// Generate all viable strategy candidates for the given failure.
    ///
    /// Spec §8.1 steps 4-5: 修正候補生成 → StrategySelector
    ///
    /// `replan_used`: pass `true` after the first Replan has been selected in
    /// this run.  Spec §6.1: replan は最大1回.
    ///
    /// `mode`: In `ExecutionMode::Proposal`, all strategy generation is
    /// disabled (Spec DBM-EXPLOSION-FIX-TIER1-SPEC §6.2–6.3).
    pub fn generate_candidates(
        &self,
        plan: &CodeIrProgram,
        failure: &FailureContext,
        history: &ExecutionHistory,
        replan_used: bool,
        mode: ExecutionMode,
    ) -> Vec<StrategyCandidate> {
        // Spec §6.2: In Proposal mode, strategy recursion is fully stopped.
        // Retry / Repair / Replan generation is prohibited.  Max depth = 0.
        if mode == ExecutionMode::Proposal {
            return vec![];
        }

        // Safety violations must not be retried under any strategy.
        if failure.error.is_safety_violation() {
            return vec![StrategyCandidate::abort()];
        }

        let mut candidates: Vec<StrategyCandidate> = Vec::new();

        // ── Retry (same plan) ─────────────────────────────────────────────────
        // Spec §8.2: Retry — only if we have not already seen this plan fail.
        if !history.has_failed(plan) {
            candidates.push(StrategyCandidate::retry(plan.clone()));
        }

        // ── Repair (local modification) ───────────────────────────────────────
        if self.allow_repair {
            if let Some(repaired) = self.repair(plan, failure) {
                // Only include if the repaired plan is different from the original
                // and hasn't already failed.
                if repaired.plan != *plan && !history.has_failed(&repaired.plan) {
                    candidates.push(repaired);
                }
            }
        }

        // ── Replan (full regeneration) ────────────────────────────────────────
        // Spec §6.1: replan は最大1回 — blocked after the first use.
        if self.allow_replan && !replan_used {
            let replanned = self.replan(plan, failure);
            if !history.has_failed(&replanned) {
                candidates.push(StrategyCandidate::replan(replanned));
            }
        }

        // If nothing viable was generated, abort.
        if candidates.is_empty() {
            candidates.push(StrategyCandidate::abort());
        }

        candidates
    }

    // ── Repair strategies ─────────────────────────────────────────────────────

    fn repair(&self, plan: &CodeIrProgram, failure: &FailureContext) -> Option<StrategyCandidate> {
        let phase = failure.step_id.phase();
        match &failure.error {
            FailureKind::ExecutionError { phase: p } if p == "build" || phase == "build" => {
                self.repair_build(plan)
            }
            FailureKind::ExecutionError { phase: p }
                if p == "dependency" || phase == "dependency" =>
            {
                self.repair_dependency(plan)
            }
            FailureKind::ExecutionError { phase: p } if p == "run" || phase == "run" => {
                self.repair_run(plan)
            }
            FailureKind::Timeout { .. } => self.repair_timeout(plan, phase),
            _ => None,
        }
    }

    /// Spec §10.1 IR部分修正: Build phase repair strategies.
    fn repair_build(&self, plan: &CodeIrProgram) -> Option<StrategyCandidate> {
        let mut repaired = plan.clone();

        // Strategy A: if build commands exist, insert a clean step before them.
        if !plan.build_plan.build_commands.is_empty() {
            let clean_cmd = match plan.language {
                TargetLanguage::Rust => "cargo clean".to_string(),
                TargetLanguage::Python => "find . -name '*.pyc' -delete".to_string(),
                TargetLanguage::TypeScript => "rm -rf node_modules/.cache".to_string(),
                TargetLanguage::Other(_) => return None,
            };
            // step挿入: prepend clean step
            repaired.build_plan.build_commands.insert(0, clean_cmd);
            return Some(StrategyCandidate::repair(
                repaired,
                "Prepend clean step before build to resolve stale artifact failures",
            ));
        }
        None
    }

    /// Spec §10.1 IR部分修正: Dependency phase repair strategies.
    fn repair_dependency(&self, plan: &CodeIrProgram) -> Option<StrategyCandidate> {
        let mut repaired = plan.clone();

        let update_cmd = match plan.language {
            TargetLanguage::Rust => "cargo update",
            TargetLanguage::Python => "pip install --upgrade pip",
            TargetLanguage::TypeScript => "npm install --legacy-peer-deps",
            TargetLanguage::Other(_) => return None,
        };
        // step挿入: prepend update/upgrade command
        repaired
            .dependency_plan
            .install_commands
            .insert(0, update_cmd.to_string());
        Some(StrategyCandidate::repair(
            repaired,
            "Prepend dependency update step to resolve resolution failures",
        ))
    }

    /// Spec §10.1 IR部分修正: Run phase repair strategies.
    fn repair_run(&self, plan: &CodeIrProgram) -> Option<StrategyCandidate> {
        let mut repaired = plan.clone();

        // parameter変更: strip any potentially conflicting flags and retry.
        repaired.run_plan.run_commands = plan
            .run_plan
            .run_commands
            .iter()
            .map(|cmd| {
                // Remove --release if present (might mismatch build output).
                cmd.replace("--release", "")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|cmd| !cmd.is_empty())
            .collect();

        if repaired.run_plan.run_commands != plan.run_plan.run_commands {
            Some(StrategyCandidate::repair(
                repaired,
                "Remove potentially conflicting flags from run commands",
            ))
        } else {
            None
        }
    }

    /// Spec §10.1 step削除: Timeout repair — remove the last command in the phase.
    fn repair_timeout(&self, plan: &CodeIrProgram, phase: &str) -> Option<StrategyCandidate> {
        let mut repaired = plan.clone();
        let removed = match phase {
            "build" => {
                if repaired.build_plan.build_commands.len() > 1 {
                    repaired.build_plan.build_commands.pop();
                    true
                } else {
                    false
                }
            }
            "test" => {
                if repaired.test_plan.test_commands.len() > 1 {
                    repaired.test_plan.test_commands.pop();
                    true
                } else {
                    false
                }
            }
            _ => false,
        };
        if removed {
            Some(StrategyCandidate::repair(
                repaired,
                format!("Remove last {phase} step to reduce timeout risk"),
            ))
        } else {
            None
        }
    }

    // ── Replan strategy ───────────────────────────────────────────────────────

    /// Generate a minimal alternative plan.
    ///
    /// Spec §8.2: Replan — keeps language/framework but resets command lists
    /// to language defaults.
    fn replan(&self, plan: &CodeIrProgram, _failure: &FailureContext) -> CodeIrProgram {
        let (dep_cmds, build_cmds, run_cmds, test_cmds) =
            default_commands_for(&plan.language, plan.framework.as_deref());

        ExecutionPlan {
            language: plan.language.clone(),
            framework: plan.framework.clone(),
            project_root: plan.project_root.clone(),
            dependency_plan: DependencyPlan {
                manifest_file: plan.dependency_plan.manifest_file.clone(),
                dependencies: plan.dependency_plan.dependencies.clone(),
                install_commands: dep_cmds,
            },
            build_plan: BuildPlan {
                build_commands: build_cmds,
            },
            run_plan: RunPlan {
                run_commands: run_cmds,
            },
            test_plan: TestPlan {
                test_files: plan.test_plan.test_files.clone(),
                test_commands: test_cmds,
            },
        }
    }
}

/// Language-specific default command lists for replan.
fn default_commands_for(
    lang: &TargetLanguage,
    _framework: Option<&str>,
) -> (Vec<String>, Vec<String>, Vec<String>, Vec<String>) {
    match lang {
        TargetLanguage::Rust => (
            vec![],
            vec!["cargo build".to_string()],
            vec![],
            vec!["cargo test".to_string()],
        ),
        TargetLanguage::Python => (
            vec!["pip install -r requirements.txt".to_string()],
            vec![],
            vec!["python main.py".to_string()],
            vec!["pytest".to_string()],
        ),
        TargetLanguage::TypeScript => (
            vec!["npm install".to_string()],
            vec!["npm run build".to_string()],
            vec!["node dist/index.js".to_string()],
            vec!["npm test".to_string()],
        ),
        TargetLanguage::Other(_) => (vec![], vec![], vec![], vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate::StrategyKind;
    use crate::failure::{FailureContext, FailureKind, StepId, StepInput};
    use crate::history::ExecutionHistory;
    use crate::types::ExecutionMode;
    use std::path::PathBuf;

    fn rust_plan_with_build(cmd: &str) -> CodeIrProgram {
        ExecutionPlan {
            language: TargetLanguage::Rust,
            framework: None,
            project_root: PathBuf::from("/tmp"),
            dependency_plan: DependencyPlan {
                manifest_file: "Cargo.toml".into(),
                dependencies: vec![],
                install_commands: vec![],
            },
            build_plan: BuildPlan {
                build_commands: vec![cmd.to_string()],
            },
            run_plan: RunPlan {
                run_commands: vec![],
            },
            test_plan: TestPlan {
                test_files: vec![],
                test_commands: vec![],
            },
        }
    }

    fn build_failure(phase: &str) -> FailureContext {
        FailureContext {
            step_id: StepId::new(phase, 0),
            error: FailureKind::ExecutionError {
                phase: phase.to_string(),
            },
            input: StepInput {
                command: vec![],
                phase: phase.to_string(),
            },
            output: None,
        }
    }

    #[test]
    fn generates_retry_first_time() {
        let planner = AdaptivePlanner::default();
        let plan = rust_plan_with_build("cargo build");
        let failure = build_failure("build");
        let history = ExecutionHistory::new();
        let candidates =
            planner.generate_candidates(&plan, &failure, &history, false, ExecutionMode::Execution);
        assert!(
            candidates
                .iter()
                .any(|c| c.strategy_kind == StrategyKind::Retry)
        );
    }

    #[test]
    fn generates_repair_for_build_failure() {
        let planner = AdaptivePlanner::default();
        let plan = rust_plan_with_build("cargo build");
        let failure = build_failure("build");
        let history = ExecutionHistory::new();
        let candidates =
            planner.generate_candidates(&plan, &failure, &history, false, ExecutionMode::Execution);
        assert!(
            candidates
                .iter()
                .any(|c| c.strategy_kind == StrategyKind::Repair)
        );
    }

    #[test]
    fn safety_violation_yields_abort_only() {
        let planner = AdaptivePlanner::default();
        let plan = rust_plan_with_build("cargo build");
        let failure = FailureContext {
            step_id: StepId::new("build", 0),
            error: FailureKind::SandboxViolation,
            input: StepInput {
                command: vec![],
                phase: "build".into(),
            },
            output: None,
        };
        let history = ExecutionHistory::new();
        let candidates =
            planner.generate_candidates(&plan, &failure, &history, false, ExecutionMode::Execution);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].strategy_kind, StrategyKind::Abort);
    }

    #[test]
    fn no_retry_if_already_failed() {
        let planner = AdaptivePlanner::new(false, false);
        let plan = rust_plan_with_build("cargo build");
        let failure = build_failure("build");
        let mut history = ExecutionHistory::new();

        // Record the plan as already failed.
        let fail_result = crate::types::RunResult {
            success: false,
            failure_type: Some(
                execution_stability_core::failure::failure_type::FailureType::BuildFailure,
            ),
            stdout: String::new(),
            stderr: "err".into(),
            steps: vec![],
        };
        history.add_failure(failure.clone(), &plan, &fail_result);

        let candidates =
            planner.generate_candidates(&plan, &failure, &history, false, ExecutionMode::Execution);
        // With repair+replan disabled and the plan already failed, expect Abort.
        assert!(
            candidates
                .iter()
                .all(|c| c.strategy_kind != StrategyKind::Retry)
        );
    }

    #[test]
    fn replan_blocked_after_first_use() {
        let planner = AdaptivePlanner::default();
        let plan = rust_plan_with_build("cargo build");
        let failure = build_failure("build");
        let history = ExecutionHistory::new();

        // With replan_used=true, no Replan candidate should be generated.
        let candidates =
            planner.generate_candidates(&plan, &failure, &history, true, ExecutionMode::Execution);
        assert!(
            candidates
                .iter()
                .all(|c| c.strategy_kind != StrategyKind::Replan),
            "Replan must be suppressed when replan_used=true"
        );
    }

    #[test]
    fn replan_generates_rust_defaults() {
        let planner = AdaptivePlanner::default();
        let plan = rust_plan_with_build("custom build cmd");
        let failure = build_failure("build");
        let replanned = planner.replan(&plan, &failure);
        assert_eq!(
            replanned.build_plan.build_commands,
            vec!["cargo build".to_string()]
        );
    }
}
