use crate::candidate::StrategyKind;
use crate::convergence::{ConvergenceGuard, FailureSignature};
use crate::failure::StrategyFailureAnalyzer;
use crate::history::{ExecutionHistory, plan_checksum};
use crate::limits::Limits;
use crate::planner::AdaptivePlanner;
use crate::policy::StrategyPolicy;
use crate::selector::StrategySelector;
use crate::trace::{StrategyOutcome, StrategyTrace};
use crate::types::{ExecutionMode, Intent, RunIntegrator, StrategyInput, StrategyOutput};
use std::time::{SystemTime, UNIX_EPOCH};

// ── StrategyEngine ────────────────────────────────────────────────────────────

/// The Phase D.1 convergent strategy engine.
///
/// Sits between the Planner and the RunIntegrator:
///
/// ```text
/// Intent + initial plan
///       ↓
/// StrategyEngine  (Phase D / D.1)
///       ↓  retry / repair / replan up to max_retries
/// RunIntegrator   (Phase C + C.5)
///       ↓
/// ExecutionResult
/// ```
///
/// Spec §4 アーキテクチャ / §8 戦略アルゴリズム / §11 収束保証アルゴリズム
///
/// ## Convergent Algorithm (spec §11)
///
/// ```text
/// for i in 0..policy.max_retries {
///     if visited_plan.contains(plan.sig()) { break; }
///     result = run(plan)
///     if result.success { return Success; }
///     failure = analyze(result)
///     if visited_failure.contains(failure.sig()) { continue; }
///     visited_failure.insert(failure.sig()); visited_plan.insert(plan.sig());
///     if is_abort(failure) { return Abort; }
///     candidates = generate_candidates(failure)
///     candidates = filter_unvisited(candidates)
///     if candidates.is_empty() { break; }
///     plan = select_best(candidates)
/// }
/// → Fallback
/// ```
pub struct StrategyEngine {
    pub policy: StrategyPolicy,
    pub analyzer: StrategyFailureAnalyzer,
    pub planner: AdaptivePlanner,
    pub selector: StrategySelector,
    pub limits: Limits,
}

impl StrategyEngine {
    pub fn new(policy: StrategyPolicy) -> Self {
        let deterministic = policy.deterministic;
        Self {
            planner: AdaptivePlanner::new(policy.allow_repair, policy.allow_replan),
            selector: StrategySelector::new(deterministic),
            policy,
            analyzer: StrategyFailureAnalyzer::new(),
            limits: Limits::default(),
        }
    }

    /// Execute the convergent strategy loop.
    ///
    /// Returns a `StrategyOutput` in all cases — even when all attempts fail,
    /// a fallback result is returned with the strategy trace preserved.
    pub fn execute(&self, input: StrategyInput, runner: &dyn RunIntegrator) -> StrategyOutput {
        let mut plan = input.initial_plan.clone();
        let mut history = input.history.clone();
        let mut trace = StrategyTrace::new(&input.intent.description);
        let mut guard = ConvergenceGuard::new();
        let mut current_strategy = StrategyKind::Retry; // initial attempt is treated as retry

        let start_ms = now_millis();

        for attempt_index in 0..self.policy.max_retries as usize {
            // Overall timeout guard (spec §12).
            if self.policy.timeout_ms > 0
                && now_millis().saturating_sub(start_ms) > self.policy.timeout_ms
            {
                trace.finish(StrategyOutcome::Fallback {
                    reason: "overall timeout exceeded".to_string(),
                });
                return StrategyOutput {
                    selected_plan: plan,
                    strategy_trace: trace,
                    success: false,
                };
            }

            // Spec §11: if visited_plan.contains(plan.sig()) { break; }
            if guard.is_plan_visited(&plan) {
                break;
            }

            // ── Run ──────────────────────────────────────────────────────────
            let ts = now_millis();
            let result = runner.run(&plan);
            let cs = plan_checksum(&plan);

            if result.success {
                history.add_success(&plan, &result);
                trace.record(
                    attempt_index,
                    current_strategy,
                    cs,
                    true,
                    None,
                    ts,
                    result.stdout.clone(),
                    result.stderr.clone(),
                );
                trace.finish(StrategyOutcome::Success);
                return StrategyOutput {
                    selected_plan: plan,
                    strategy_trace: trace,
                    success: true,
                };
            }

            // ── Failure path ─────────────────────────────────────────────────
            let failure_ctx = self.analyzer.analyze(&result);

            trace.record(
                attempt_index,
                current_strategy.clone(),
                cs,
                false,
                failure_ctx.clone(),
                ts,
                result.stdout.clone(),
                result.stderr.clone(),
            );

            let failure = match failure_ctx {
                Some(f) => f,
                None => {
                    trace.finish(StrategyOutcome::Fallback {
                        reason: "execution failed with no analysable failure context".to_string(),
                    });
                    return StrategyOutput {
                        selected_plan: plan,
                        strategy_trace: trace,
                        success: false,
                    };
                }
            };

            // Spec §11: if visited_failure.contains(failure.sig()) { continue; }
            let sig = FailureSignature::from_failure(&failure);
            if guard.is_failure_visited(&sig) {
                // Same failure pattern already processed — consume this iteration
                // and try the same plan again (max_retries cap ensures termination).
                history.add_failure(failure, &plan, &result);
                continue;
            }

            // Spec §11: visited_failure.insert(); visited_plan.insert();
            guard.mark_failure_visited(&sig);
            guard.mark_plan_visited(&plan);
            history.add_failure(failure.clone(), &plan, &result);

            // Spec §11 / §13.2: safety violations abort immediately.
            if failure.error.is_safety_violation() {
                trace.finish(StrategyOutcome::Aborted {
                    reason: format!("safety violation: {:?}", failure.error),
                });
                return StrategyOutput {
                    selected_plan: plan,
                    strategy_trace: trace,
                    success: false,
                };
            }

            // Spec §11: candidates = generate_candidates(failure)
            // Spec §6.2: always pass ExecutionMode::Execution here — strategy
            // recursion in Proposal mode is blocked at the planner level.
            let replan_used = !guard.replan_allowed();
            let candidates = self.planner.generate_candidates(
                &plan,
                &failure,
                &history,
                replan_used,
                ExecutionMode::Execution,
            );
            println!("[TRACE][COUNT][CANDIDATES_RAW] {}", candidates.len());

            // Spec §11: candidates = filter_unvisited(candidates)
            let mut candidates = guard.filter_unvisited(candidates);
            candidates.sort_by(|a, b| {
                b.score()
                    .partial_cmp(&a.score())
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            candidates.truncate(self.limits.max_candidates);
            println!("[TRACE][COUNT][AFTER_STRATEGY] {}", candidates.len());

            // Spec §11: if candidates.is_empty() { break; }
            if candidates.is_empty() {
                break;
            }

            // Select best candidate using history-weighted scores (spec §8).
            let chosen = if self.policy.deterministic {
                self.selector.select_best_with_history(candidates, &history)
            } else {
                self.selector
                    .select_top_k_with_history(candidates, self.policy.beam_width, &history)
                    .into_iter()
                    .next()
            };

            let chosen = match chosen {
                Some(c) => c,
                None => break,
            };

            if chosen.strategy_kind == StrategyKind::Abort {
                trace.finish(StrategyOutcome::Aborted {
                    reason: chosen.rationale.clone(),
                });
                return StrategyOutput {
                    selected_plan: plan,
                    strategy_trace: trace,
                    success: false,
                };
            }

            // Consume the replan budget when a Replan is selected (spec §6.1).
            if chosen.strategy_kind == StrategyKind::Replan {
                guard.mark_replan_used();
            }

            current_strategy = chosen.strategy_kind.clone();
            plan = chosen.plan;
        }

        // ── Fallback ──────────────────────────────────────────────────────────
        // Spec §12: 全戦略失敗 → failure集約 + best-effort結果返却 + trace保存
        let reason = build_fallback_reason(&history);
        trace.finish(StrategyOutcome::Fallback { reason });
        StrategyOutput {
            selected_plan: input.initial_plan,
            strategy_trace: trace,
            success: false,
        }
    }
}

impl Default for StrategyEngine {
    fn default() -> Self {
        Self::new(StrategyPolicy::default())
    }
}

// ── Fallback helpers ──────────────────────────────────────────────────────────

fn build_fallback_reason(history: &ExecutionHistory) -> String {
    let n = history.failure_count();
    let last = history
        .last_failure()
        .map(|f| format!("{:?}", f.error))
        .unwrap_or_else(|| "unknown".to_string());
    format!("all {n} attempts failed; last error: {last}")
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ── Convenience constructor ───────────────────────────────────────────────────

impl StrategyInput {
    pub fn new(intent: Intent, plan: crate::types::CodeIrProgram) -> Self {
        Self {
            intent,
            initial_plan: plan,
            context: crate::types::ExecutionContext::default(),
            history: ExecutionHistory::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::StrategyPolicy;
    use crate::types::{CodeIrProgram, DryRunIntegrator, FailThenSucceedIntegrator, Intent};
    use execution_core::engine::execution_plan::*;
    use std::path::PathBuf;

    fn plan() -> CodeIrProgram {
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
                build_commands: vec!["cargo build".into()],
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

    // ── Spec §11.1: 再現性 (deterministic mode) ─────────────────────────────────
    #[test]
    fn deterministic_mode_same_input_same_outcome() {
        let engine = StrategyEngine::new(StrategyPolicy::default());
        let runner = DryRunIntegrator;

        let input_a = StrategyInput::new(Intent::new("build"), plan());
        let input_b = StrategyInput::new(Intent::new("build"), plan());

        let out_a = engine.execute(input_a, &runner);
        let out_b = engine.execute(input_b, &runner);

        assert_eq!(out_a.success, out_b.success);
        assert_eq!(
            out_a.strategy_trace.final_outcome,
            out_b.strategy_trace.final_outcome
        );
    }

    // ── Spec §11.2: 改善性 (recovery from failure) ───────────────────────────────
    #[test]
    fn recovers_after_one_failure() {
        let engine = StrategyEngine::new(StrategyPolicy::default());
        let runner = FailThenSucceedIntegrator::new(1);
        let input = StrategyInput::new(Intent::new("build"), plan());
        let output = engine.execute(input, &runner);
        assert!(output.success, "engine must recover after one failure");
        assert!(output.strategy_trace.attempt_count() >= 2);
    }

    // ── Spec §11.4: 上限制御 (max_retries not exceeded) ──────────────────────────
    #[test]
    fn does_not_exceed_max_retries() {
        let policy = StrategyPolicy {
            max_retries: 3,
            ..Default::default()
        };
        let engine = StrategyEngine::new(policy);
        // Always fails
        let runner = FailThenSucceedIntegrator::new(100);
        let input = StrategyInput::new(Intent::new("build"), plan());
        let output = engine.execute(input, &runner);
        assert!(!output.success);
        assert!(
            output.strategy_trace.attempt_count() <= 3,
            "attempts={} must be ≤ max_retries=3",
            output.strategy_trace.attempt_count()
        );
    }

    // ── Spec §11.3: 安全性 (safety constraint not bypassed) ──────────────────────
    #[test]
    fn sandbox_violation_aborts_immediately() {
        use crate::types::{RunResult, StepInfo};
        use execution_stability_core::failure::failure_type::FailureType;

        struct SandboxViolatingRunner;
        impl RunIntegrator for SandboxViolatingRunner {
            fn run(&self, _plan: &CodeIrProgram) -> RunResult {
                RunResult {
                    success: false,
                    failure_type: Some(FailureType::SandboxViolation),
                    stdout: String::new(),
                    stderr: "sandbox violated".into(),
                    steps: vec![StepInfo {
                        phase: "build".into(),
                        success: false,
                        stdout: String::new(),
                        stderr: "sandbox violated".into(),
                    }],
                }
            }
        }

        let engine = StrategyEngine::new(StrategyPolicy::default());
        let input = StrategyInput::new(Intent::new("build"), plan());
        let output = engine.execute(input, &SandboxViolatingRunner);

        assert!(!output.success);
        assert!(
            matches!(
                output.strategy_trace.final_outcome,
                StrategyOutcome::Aborted { .. }
            ),
            "expected Aborted, got {:?}",
            output.strategy_trace.final_outcome
        );
        // Must abort on first violation — only one attempt recorded.
        assert_eq!(output.strategy_trace.attempt_count(), 1);
    }

    #[test]
    fn immediate_success_records_single_attempt() {
        let engine = StrategyEngine::default();
        let runner = DryRunIntegrator;
        let input = StrategyInput::new(Intent::new("build"), plan());
        let output = engine.execute(input, &runner);
        assert!(output.success);
        assert_eq!(output.strategy_trace.attempt_count(), 1);
        assert!(matches!(
            output.strategy_trace.final_outcome,
            StrategyOutcome::Success
        ));
    }
}
