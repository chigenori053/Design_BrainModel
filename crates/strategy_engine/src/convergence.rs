use crate::candidate::{StrategyCandidate, StrategyKind};
use crate::failure::{FailureContext, FailureKind, StepId};
use crate::history::plan_checksum;
use crate::types::CodeIrProgram;
use execution_hardening::Checksum;
use std::collections::HashSet;

// ── ExecutionOp ───────────────────────────────────────────────────────────────

/// The concrete operation being executed.
///
/// Git operations are modeled as typed operations instead of generic external
/// commands so the runtime can enforce UX and safety constraints centrally.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ExecutionOp {
    RuntimePhase(String),
    GitStatus,
    GitDiff,
    GitAdd { path: String },
    GitCommit { message: String },
}

impl ExecutionOp {
    /// Derive from a failure context: prefer first command word, then phase name.
    pub fn from_failure(failure: &FailureContext) -> Self {
        let op = failure
            .input
            .command
            .first()
            .and_then(|c| c.split_whitespace().next())
            .unwrap_or(&failure.input.phase);
        Self::RuntimePhase(op.to_string())
    }

    pub fn from_phase(phase: &str) -> Self {
        Self::RuntimePhase(phase.to_string())
    }

    pub fn label(&self) -> String {
        match self {
            Self::RuntimePhase(phase) => phase.clone(),
            Self::GitStatus => "git status".to_string(),
            Self::GitDiff => "git diff".to_string(),
            Self::GitAdd { path } => format!("git add {path}"),
            Self::GitCommit { .. } => "git commit".to_string(),
        }
    }
}

// ── FailureSignature ──────────────────────────────────────────────────────────

/// A compact, stable fingerprint for a specific failure scenario.
///
/// Prevents the strategy loop from re-trying strategies that have already
/// been generated for an identical failure pattern.
///
/// Spec §4 FailureSignature
/// `signature = hash(failure_kind + step_id + op + target)`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FailureSignature {
    pub failure_kind: FailureKind,
    pub step_id: StepId,
    pub op: ExecutionOp,
    /// Compact hash of the "target" — the failure message / content.
    /// Spec §4.1: `target_hash: u64`
    pub target_hash: u64,
}

impl FailureSignature {
    /// Build a `FailureSignature` from a `FailureContext`.
    ///
    /// Spec §4.2: `signature = hash(failure_kind + step_id + op + target)`
    pub fn from_failure(failure: &FailureContext) -> Self {
        let op = ExecutionOp::from_failure(failure);
        // Use the first 128 chars of stderr as the "target" to distinguish
        // failure causes without being sensitive to long stack traces.
        let stderr_snip = failure
            .output
            .as_ref()
            .map(|o| &o.stderr as &str)
            .unwrap_or("")
            .chars()
            .take(128)
            .collect::<String>();
        let target_hash = checksum_to_u64(&Checksum::of_str(&stderr_snip));
        Self {
            failure_kind: failure.error.clone(),
            step_id: failure.step_id.clone(),
            op,
            target_hash,
        }
    }

    /// Compute a stable u64 key for this signature.
    ///
    /// Used as the key in `HashSet<u64>`.  Spec §4.2 deterministic hash.
    pub fn hash_key(&self) -> u64 {
        // Combine all fields into a deterministic string and hash it.
        let s = format!(
            "{:?}:{}:{}:{}:{}",
            self.failure_kind,
            self.step_id.phase,
            self.step_id.command_index,
            self.op.label(),
            self.target_hash,
        );
        checksum_to_u64(&Checksum::of_str(&s))
    }
}

/// Extract the low 8 bytes of a blake3 Checksum as a u64 (little-endian).
fn checksum_to_u64(c: &Checksum) -> u64 {
    let b = c.as_bytes();
    u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
}

// ── StrategyState ─────────────────────────────────────────────────────────────

/// Mutable per-run state threaded through the strategy loop.
///
/// Spec §6.2 StrategyState
#[derive(Debug, Default, Clone)]
pub struct StrategyState {
    /// Whether a full Replan has already been used in this run.
    ///
    /// Spec §6.1: replan は最大1回.  After the first replan, only Repair is
    /// allowed (spec §6.1: replan後は repair のみ許可).
    pub replan_used: bool,
}

impl StrategyState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if replan is still available.
    pub fn replan_allowed(&self) -> bool {
        !self.replan_used
    }

    /// Mark replan as consumed.
    pub fn mark_replan_used(&mut self) {
        self.replan_used = true;
    }
}

// ── ConvergenceGuard ──────────────────────────────────────────────────────────

/// Enforces the Phase D.1 convergence guarantee by maintaining two monotonically
/// growing visited sets.
///
/// Spec §7 Strategy Graph制約:
/// - `visited_plans`    — set of PlanSignatures already executed (spec §5)
/// - `visited_failures` — set of FailureSignature hashes already processed (spec §4)
///
/// Convergence proof (spec §12):
/// - Both sets are finite and can only grow.
/// - The algorithm terminates when max_retries is exhausted, a success is returned,
///   an abort is triggered, or no new candidates are available.
/// - Therefore the loop **always terminates** in finite steps.
#[derive(Debug, Default)]
pub struct ConvergenceGuard {
    /// Spec §5.3: `HashSet<PlanSignature>` — stored as raw checksum bytes.
    visited_plans: HashSet<[u8; 32]>,
    /// Spec §4.4: `HashSet<FailureSignature>` — stored as u64 hash key.
    visited_failures: HashSet<u64>,
    /// Replan and other mutable strategy state.
    pub state: StrategyState,
}

impl ConvergenceGuard {
    pub fn new() -> Self {
        Self::default()
    }

    // ── Plan tracking (spec §5) ───────────────────────────────────────────────

    /// `true` when `plan` was already executed in this run.
    ///
    /// Spec §5.2: 同一PlanSignatureの再実行禁止.
    pub fn is_plan_visited(&self, plan: &CodeIrProgram) -> bool {
        self.visited_plans.contains(plan_checksum(plan).as_bytes())
    }

    /// Mark `plan` as executed.
    pub fn mark_plan_visited(&mut self, plan: &CodeIrProgram) {
        self.visited_plans.insert(*plan_checksum(plan).as_bytes());
    }

    // ── Failure tracking (spec §4) ────────────────────────────────────────────

    /// `true` when an equivalent failure was already processed.
    ///
    /// Spec §4.3: 同一signatureの再試行禁止.
    pub fn is_failure_visited(&self, sig: &FailureSignature) -> bool {
        self.visited_failures.contains(&sig.hash_key())
    }

    /// Record that this failure signature has been processed.
    pub fn mark_failure_visited(&mut self, sig: &FailureSignature) {
        self.visited_failures.insert(sig.hash_key());
    }

    // ── Candidate filtering (spec §11) ────────────────────────────────────────

    /// Remove candidates whose plan was already visited.
    ///
    /// Spec §11: `candidates = filter_unvisited(candidates)`.
    /// Abort candidates are always kept regardless.
    pub fn filter_unvisited(&self, candidates: Vec<StrategyCandidate>) -> Vec<StrategyCandidate> {
        candidates
            .into_iter()
            .filter(|c| {
                c.strategy_kind == StrategyKind::Abort
                    || !self
                        .visited_plans
                        .contains(plan_checksum(&c.plan).as_bytes())
            })
            .collect()
    }

    // ── Replan state ──────────────────────────────────────────────────────────

    /// Whether another replan is permitted.  Spec §6.1.
    pub fn replan_allowed(&self) -> bool {
        self.state.replan_allowed()
    }

    /// Consume the one-shot replan budget.
    pub fn mark_replan_used(&mut self) {
        self.state.mark_replan_used();
    }

    // ── Diagnostics ───────────────────────────────────────────────────────────

    pub fn visited_plan_count(&self) -> usize {
        self.visited_plans.len()
    }

    pub fn visited_failure_count(&self) -> usize {
        self.visited_failures.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::failure::{FailureContext, FailureKind, StepId, StepInput};
    use execution_core::engine::execution_plan::*;
    use std::path::PathBuf;

    fn dummy_plan(tag: &str) -> CodeIrProgram {
        ExecutionPlan {
            language: TargetLanguage::Rust,
            framework: None,
            project_root: PathBuf::from(format!("/tmp/{tag}")),
            dependency_plan: DependencyPlan {
                manifest_file: "Cargo.toml".into(),
                dependencies: vec![],
                install_commands: vec![],
            },
            build_plan: BuildPlan {
                build_commands: vec![tag.to_string()],
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

    fn build_failure(stderr: &str) -> FailureContext {
        FailureContext {
            step_id: StepId::new("build", 0),
            error: FailureKind::ExecutionError {
                phase: "build".into(),
            },
            input: StepInput {
                command: vec!["cargo".into(), "build".into()],
                phase: "build".into(),
            },
            output: Some(crate::failure::StepOutput {
                stdout: String::new(),
                stderr: stderr.to_string(),
            }),
        }
    }

    // ── Spec §13.1: ループ防止 ──────────────────────────────────────────────────
    #[test]
    fn same_failure_not_revisited() {
        let mut guard = ConvergenceGuard::new();
        let failure = build_failure("link error");
        let sig = FailureSignature::from_failure(&failure);

        assert!(!guard.is_failure_visited(&sig));
        guard.mark_failure_visited(&sig);
        assert!(
            guard.is_failure_visited(&sig),
            "failure must be seen as visited after marking"
        );
    }

    #[test]
    fn same_plan_not_revisited() {
        let mut guard = ConvergenceGuard::new();
        let plan = dummy_plan("a");

        assert!(!guard.is_plan_visited(&plan));
        guard.mark_plan_visited(&plan);
        assert!(guard.is_plan_visited(&plan));
    }

    #[test]
    fn different_plans_tracked_independently() {
        let mut guard = ConvergenceGuard::new();
        let p1 = dummy_plan("a");
        let p2 = dummy_plan("b");

        guard.mark_plan_visited(&p1);
        assert!(guard.is_plan_visited(&p1));
        assert!(!guard.is_plan_visited(&p2));
    }

    #[test]
    fn filter_unvisited_removes_visited_plans() {
        let mut guard = ConvergenceGuard::new();
        let p1 = dummy_plan("x");
        let p2 = dummy_plan("y");

        guard.mark_plan_visited(&p1);

        use crate::candidate::StrategyCandidate;
        let candidates = vec![
            StrategyCandidate::retry(p1.clone()), // visited → removed
            StrategyCandidate::retry(p2.clone()), // unvisited → kept
        ];
        let filtered = guard.filter_unvisited(candidates);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].plan, p2);
    }

    #[test]
    fn abort_candidate_always_passes_filter() {
        let mut guard = ConvergenceGuard::new();
        // Abort plan is a placeholder — even if it looks like a visited plan,
        // Abort candidates must always pass through.
        let candidates = vec![StrategyCandidate::abort()];
        // Mark the abort's placeholder plan as visited.
        guard.mark_plan_visited(&candidates[0].plan);
        let filtered = guard.filter_unvisited(candidates);
        assert_eq!(filtered.len(), 1, "Abort must always pass the filter");
    }

    #[test]
    fn replan_allowed_once() {
        let mut guard = ConvergenceGuard::new();
        assert!(guard.replan_allowed());
        guard.mark_replan_used();
        assert!(
            !guard.replan_allowed(),
            "replan must be blocked after first use"
        );
    }

    #[test]
    fn failure_signature_is_deterministic() {
        let f = build_failure("error: linker not found");
        let s1 = FailureSignature::from_failure(&f);
        let s2 = FailureSignature::from_failure(&f);
        assert_eq!(s1.hash_key(), s2.hash_key());
    }

    #[test]
    fn different_stderr_produces_different_signature() {
        let f1 = build_failure("error: missing file");
        let f2 = build_failure("error: type mismatch");
        let s1 = FailureSignature::from_failure(&f1);
        let s2 = FailureSignature::from_failure(&f2);
        assert_ne!(s1.hash_key(), s2.hash_key());
    }
}
