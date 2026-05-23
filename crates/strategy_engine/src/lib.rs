//! # strategy_engine
//!
//! Phase D: Strategy / Adaptive Execution layer for the DBM pipeline.
//!
//! ## Position in the stack
//!
//! ```text
//! Intent + initial plan
//!       ↓
//! StrategyEngine          ← Phase D (this crate)
//!       ↓
//! RunIntegrator           ← Phase C / C.5 (execution_stability_core)
//!       ↓
//! ExecutionResult
//! ```
//!
//! ## Core design (spec §3)
//!
//! | Principle                | Guarantee                                            |
//! |--------------------------|------------------------------------------------------|
//! | Strategy over Execution  | Execution layer is fixed; only strategy varies       |
//! | Bounded Adaptation       | `max_retries` caps exploration — no infinite loops   |
//! | Failure-driven Optim.    | Every failure is recorded and informs next strategy  |
//! | Deterministic Strategy   | Same input → same strategy (deterministic mode)      |
//!
//! ## Algorithm (spec §8.3)
//!
//! ```text
//! for i in 0..policy.max_retries {
//!     result = runner.run(plan)
//!     if success → return
//!     failure = analyze(result)
//!     candidates = planner.generate(failure, history)
//!     plan = selector.select_best(candidates)
//! }
//! → fallback
//! ```
//!
//! ## Module overview
//!
//! - [`types`] — `CodeIrProgram`, `Intent`, `ExecutionContext`, `StrategyInput/Output`,
//!   `RunIntegrator` trait + adapters
//! - [`policy`]    — `StrategyPolicy` (max_retries, beam_width, allow_repair, …)
//! - [`failure`]   — `FailureKind`, `FailureContext`, `StrategyFailureAnalyzer`
//! - [`history`]   — `ExecutionHistory` (deduplication + pattern learning)
//! - [`candidate`] — `StrategyCandidate`, `StrategyKind` (Retry/Repair/Replan/Abort)
//! - [`selector`]  — `StrategySelector` (score = expected_gain - risk - cost)
//! - [`planner`]   — `AdaptivePlanner` (IR partial modification, step insert/delete)
//! - [`trace`]     — `StrategyTrace`, `StrategyAttempt`, `StrategyOutcome`
//! - [`engine`]    — `StrategyEngine` (main entry point)

pub mod candidate;
pub mod convergence;
pub mod engine;
pub mod failure;
pub mod history;
pub mod limits;
pub mod planner;
pub mod policy;
pub mod proposal;
pub mod selector;
pub mod trace;
pub mod types;

// Convenience re-exports
pub use candidate::{StrategyCandidate, StrategyKind};
pub use convergence::{ConvergenceGuard, ExecutionOp, FailureSignature, StrategyState};
pub use engine::StrategyEngine;
pub use failure::{FailureContext, FailureKind, StepId, StrategyFailureAnalyzer};
pub use history::ExecutionHistory;
pub use limits::Limits;
pub use planner::AdaptivePlanner;
pub use policy::StrategyPolicy;
pub use proposal::{
    EffectKind, ExecutionPlanCandidate, ExpectedEffect, ImpactLevel, MAX_CANDIDATES,
    ResolvedTarget, Risk, RiskLevel, generate_candidates, generate_candidates_from_intent,
    generate_candidates_from_intent_with_limits, generate_candidates_with_limits,
    requires_clarification,
};
pub use selector::StrategySelector;
pub use trace::{StrategyAttempt, StrategyAttemptInput, StrategyOutcome, StrategyTrace};
pub use types::{
    Action, CodeIrProgram, DryRunIntegrator, ExecutionContext, ExecutionMode,
    FIXED_GIT_COMMIT_MESSAGE, FailThenSucceedIntegrator, HardenedRunIntegrator, Intent,
    RunIntegrator, RunResult, StrategyInput, StrategyOutput,
};
