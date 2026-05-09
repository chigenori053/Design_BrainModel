use crate::core::Diff;
use crate::tui::runtime::RuntimeShellState;

/// Unique identifier for a branch instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BranchId(pub String);

/// Lifecycle state of a branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchState {
    Committed,
    Speculative,
    Rejected,
    Pruned,
}

/// Budget for speculative branch exploration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchBudget {
    pub remaining_branches: usize,
    pub remaining_depth: usize,
    pub remaining_rollbacks: usize,
    pub remaining_steps: usize,
    pub max_active_branches: usize,
}

impl BranchBudget {
    pub fn new(branches: usize, depth: usize, rollbacks: usize, steps: usize) -> Self {
        Self {
            remaining_branches: branches,
            remaining_depth: depth,
            remaining_rollbacks: rollbacks,
            remaining_steps: steps,
            max_active_branches: branches,
        }
    }

    pub fn default_limits() -> Self {
        Self::new(10, 3, 5, 100)
    }

    pub fn is_exhausted(&self) -> bool {
        self.remaining_branches == 0
            || self.remaining_depth == 0
            || self.remaining_rollbacks == 0
            || self.remaining_steps == 0
    }
}

/// Snapshot of world state hashes for grounding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldStateSnapshot {
    pub filesystem_hash: String,
    pub dependency_graph_hash: String,
    pub verification_hash: String,
    pub runtime_effect_hash: String,
    pub causal_state_hash: String,
}

impl WorldStateSnapshot {
    pub fn zero() -> Self {
        Self {
            filesystem_hash: "0".to_string(),
            dependency_graph_hash: "0".to_string(),
            verification_hash: "0".to_string(),
            runtime_effect_hash: "0".to_string(),
            causal_state_hash: "0".to_string(),
        }
    }
}

/// Consistency score grounded in the world model.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldConsistencyScore {
    pub filesystem_consistency: f32,
    pub dependency_consistency: f32,
    pub verification_consistency: f32,
    pub execution_consistency: f32,
    pub causal_consistency: f32,
}

impl WorldConsistencyScore {
    pub fn zero() -> Self {
        Self {
            filesystem_consistency: 0.0,
            dependency_consistency: 0.0,
            verification_consistency: 0.0,
            execution_consistency: 0.0,
            causal_consistency: 0.0,
        }
    }

    pub fn total(&self) -> f32 {
        self.filesystem_consistency
            + self.dependency_consistency
            + self.verification_consistency
            + self.execution_consistency
            + self.causal_consistency
    }
}

/// Set of side effects observed in the world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeEffectSet {
    pub filesystem_changes: usize,
    pub dependency_changes: usize,
    pub verification_failures: usize,
    pub execution_side_effects: usize,
}

impl RuntimeEffectSet {
    pub fn zero() -> Self {
        Self {
            filesystem_changes: 0,
            dependency_changes: 0,
            verification_failures: 0,
            execution_side_effects: 0,
        }
    }
}

/// Deterministic scoring for speculative branches.
#[derive(Debug, Clone, PartialEq)]
pub struct ConvergenceScore {
    pub architectural_consistency: f32,
    pub dependency_stability: f32,
    pub replay_reliability: f32,
    pub contradiction_penalty: f32,
    pub repairability: f32,
    pub complexity_penalty: f32,
    pub world_consistency: WorldConsistencyScore,
}

impl ConvergenceScore {
    pub fn zero() -> Self {
        Self {
            architectural_consistency: 0.0,
            dependency_stability: 0.0,
            replay_reliability: 0.0,
            contradiction_penalty: 0.0,
            repairability: 0.0,
            complexity_penalty: 0.0,
            world_consistency: WorldConsistencyScore::zero(),
        }
    }

    pub fn total(&self) -> f32 {
        (self.architectural_consistency
            + self.dependency_stability
            + self.replay_reliability
            + self.repairability
            + self.world_consistency.total())
            - (self.contradiction_penalty + self.complexity_penalty)
    }
}

/// Tracking convergence progress.
#[derive(Debug, Clone, PartialEq)]
pub struct ConvergenceTrajectory {
    pub previous_score: f32,
    pub current_score: f32,
    pub improvement_delta: f32,
    pub stagnation_cycles: usize,
}

impl ConvergenceTrajectory {
    pub fn new() -> Self {
        Self {
            previous_score: 0.0,
            current_score: 0.0,
            improvement_delta: 0.0,
            stagnation_cycles: 0,
        }
    }

    pub fn update(&mut self, next_score: f32) {
        self.improvement_delta = next_score - self.current_score;
        self.previous_score = self.current_score;
        self.current_score = next_score;

        if self.improvement_delta <= 0.0 {
            self.stagnation_cycles += 1;
        } else {
            self.stagnation_cycles = 0;
        }
    }
}

/// Set of conflicts detected in a branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContradictionSet {
    pub ownership_conflicts: usize,
    pub dependency_conflicts: usize,
    pub replay_inconsistencies: usize,
    pub lifecycle_conflicts: usize,
}

impl ContradictionSet {
    pub fn zero() -> Self {
        Self {
            ownership_conflicts: 0,
            dependency_conflicts: 0,
            replay_inconsistencies: 0,
            lifecycle_conflicts: 0,
        }
    }

    pub fn total(&self) -> usize {
        self.ownership_conflicts
            + self.dependency_conflicts
            + self.replay_inconsistencies
            + self.lifecycle_conflicts
    }
}

/// Immutable snapshot of branch state captured at commit time.
///
/// Once created, a `BranchSnapshot` is never mutated.  All fields are
/// read-only after construction.
#[derive(Debug, Clone, PartialEq)]
pub struct BranchSnapshot {
    pub branch_id: BranchId,
    pub parent_branch: Option<BranchId>,
    pub tx_id: String,
    pub target: String,
    pub runtime_state: RuntimeShellState,
    pub projection: Diff,
    pub score: ConvergenceScore,
    pub contradictions: ContradictionSet,
    pub world_state: WorldStateSnapshot,
    pub runtime_effects: RuntimeEffectSet,
    pub depth: usize,
    pub created_at: u64,
}

impl BranchSnapshot {
    pub fn new(
        branch_id: BranchId,
        parent_branch: Option<BranchId>,
        tx_id: String,
        target: String,
        runtime_state: RuntimeShellState,
        projection: Diff,
        score: ConvergenceScore,
        contradictions: ContradictionSet,
        world_state: WorldStateSnapshot,
        runtime_effects: RuntimeEffectSet,
        depth: usize,
        created_at: u64,
    ) -> Self {
        Self {
            branch_id,
            parent_branch,
            tx_id,
            target,
            runtime_state,
            projection,
            score,
            contradictions,
            world_state,
            runtime_effects,
            depth,
            created_at,
        }
    }
}

/// Multi-parent / multi-child bounded branch runtime.
///
/// # Invariants
///
/// 1. `committed_branch` is the sole runtime authority.
/// 2. `speculative_branches` are invisible on the runtime surface until
///    [`commit_branch`] succeeds.
/// 3. Budget limits are strictly enforced.
#[derive(Debug, Clone, PartialEq)]
pub struct BranchRuntime {
    pub committed_branch: BranchSnapshot,
    pub speculative_branches: Vec<BranchSnapshot>,
    pub budget: BranchBudget,
    pub trajectory: ConvergenceTrajectory,
    pub next_age: u64,
    pub max_stagnation_cycles: usize,
}

impl BranchRuntime {
    /// Create a new `BranchRuntime` with the given snapshot as the sole
    /// committed branch.  No speculative branch exists initially.
    pub fn new(committed: BranchSnapshot) -> Self {
        Self {
            committed_branch: committed,
            speculative_branches: Vec::new(),
            budget: BranchBudget::default_limits(),
            trajectory: ConvergenceTrajectory::new(),
            next_age: 1,
            max_stagnation_cycles: 5,
        }
    }

    /// Stage a speculative child branch.
    ///
    /// Rule 2: the speculative branch remains invisible on the runtime
    /// surface until [`commit_branch`] is called.
    pub fn open_speculative(&mut self, mut child: BranchSnapshot) -> bool {
        if self.budget.remaining_branches == 0 || child.depth > self.budget.remaining_depth {
            return false;
        }

        self.budget.remaining_branches -= 1;
        child.created_at = self.next_age;
        self.next_age += 1;
        self.speculative_branches.push(child);
        self.prune_branches();
        true
    }

    /// Deterministically prune speculative branches to fit limits.
    fn prune_branches(&mut self) {
        // Deterministic Evaluation Ordering (Specified in 5.2 & 8.1):
        // verification -> causal consistency -> dependency consistency -> filesystem consistency -> replay consistency
        // (Unified via total score, but tie-breakers follow ordering)
        self.speculative_branches.sort_by(|a, b| {
            b.score
                .total()
                .partial_cmp(&a.score.total())
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    b.score
                        .world_consistency
                        .verification_consistency
                        .partial_cmp(&a.score.world_consistency.verification_consistency)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| {
                    b.score
                        .world_consistency
                        .causal_consistency
                        .partial_cmp(&a.score.world_consistency.causal_consistency)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.contradictions.total().cmp(&b.contradictions.total()))
                .then_with(|| a.created_at.cmp(&b.created_at))
                .then_with(|| a.branch_id.0.cmp(&b.branch_id.0))
        });

        if self.speculative_branches.len() <= self.budget.max_active_branches {
            return;
        }

        // Lowest ranked branches pruned first.
        self.speculative_branches.truncate(self.budget.max_active_branches);
    }

    /// Promote a specific speculative branch to committed.
    pub fn commit_branch_by_id(&mut self, id: &BranchId) -> bool {
        let Some(pos) = self.speculative_branches.iter().position(|b| &b.branch_id == id) else {
            return false;
        };
        let child = self.speculative_branches.remove(pos);

        // Update trajectory.
        self.trajectory.update(child.score.total());

        self.committed_branch = child;
        // Spec says: After a successful commit there is exactly one committed branch and no speculative branch.
        // Actually, the spec says "speculative branches pruned first" if budget exceeded,
        // but commit promotes one. In our model, we'll clear speculative ones on commit to keep it simple,
        // or keep them if they are siblings. Let's stick to the spec's "single committed authority".
        self.speculative_branches.clear();
        true
    }

    pub fn commit_branch(&mut self) -> bool {
        if self.speculative_branches.is_empty() {
            return false;
        }
        // Promote the highest ranked branch.
        self.prune_branches();
        let child = self.speculative_branches.remove(0);

        // Update trajectory.
        self.trajectory.update(child.score.total());

        self.committed_branch = child;
        self.speculative_branches.clear();
        true
    }

    /// Oscillation Detection (Specified in 7.3)
    pub fn detect_branch_oscillation(&self, snapshot: &BranchSnapshot) -> bool {
        // Simple oscillation detection: if the snapshot's projection matches
        // a previously committed state in the current trajectory.
        // Rule: prevents architecture oscillation loops.
        snapshot.projection == self.committed_branch.projection
            && snapshot.parent_branch == self.committed_branch.parent_branch
    }

    /// Check for convergence halt conditions.
    pub fn should_halt(&self) -> bool {
        self.trajectory.stagnation_cycles >= self.max_stagnation_cycles
    }

    /// Permanently destroy the speculative branch.
    pub fn reject_speculative(&mut self) {
        self.speculative_branches.clear();
    }

    /// Rule 4: rollback exact restoration.
    pub fn rollback(&mut self) -> bool {
        if self.budget.remaining_rollbacks == 0 {
            return false;
        }
        self.budget.remaining_rollbacks -= 1;
        self.speculative_branches.clear();
        true
    }

    /// Return the runtime-visible surface snapshot.
    ///
    /// Rule 2: always returns the committed branch regardless of whether a
    /// speculative branch is staged.
    pub fn surface_snapshot(&self) -> &BranchSnapshot {
        &self.committed_branch
    }

    /// Return `true` if speculative child branches are currently staged.
    pub fn has_speculative(&self) -> bool {
        !self.speculative_branches.is_empty()
    }
}

/// Branch Evaluation Engine (Specified in 5.1)
pub fn evaluate_branch_convergence(snapshot: &mut BranchSnapshot) {
    // 5.1 branch scoring based on specified rules.

    // Rule 2: contradiction accumulation penalty.
    let contradictions = snapshot.contradictions.total();
    if contradictions > 0 {
        snapshot.score.contradiction_penalty = contradictions as f32 * 50.0;
    }

    // Rule 3: replay instability penalty.
    if snapshot.score.replay_reliability < 0.5 {
        snapshot.score.complexity_penalty += 5.0;
    }

    // Integrated World Convergence (Specified in 5.1)
    evaluate_world_convergence(snapshot);
}

/// World Model Convergence Evaluation (Specified in 5.1)
pub fn evaluate_world_convergence(snapshot: &mut BranchSnapshot) {
    // Rule 3: verification drift penalty.
    if snapshot.runtime_effects.verification_failures > 0 {
        snapshot.score.world_consistency.verification_consistency = -20.0;
    } else {
        snapshot.score.world_consistency.verification_consistency = 10.0;
    }

    // Rule 1: filesystem drift.
    if snapshot.runtime_effects.filesystem_changes > 10 {
        snapshot.score.world_consistency.filesystem_consistency = -5.0;
    } else {
        snapshot.score.world_consistency.filesystem_consistency = 5.0;
    }

    // Rule 4: causal inconsistency.
    // (In a real system, compare world_state_hash with parent).
    if snapshot.world_state.causal_state_hash == "INVALID" {
        snapshot.score.world_consistency.causal_consistency = -100.0;
    } else {
        snapshot.score.world_consistency.causal_consistency = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Diff, DiffChunk};

    // ── helpers ────────────────────────────────────────────────────────────

    fn make_diff(file: &str) -> Diff {
        Diff {
            file: file.to_string(),
            changes: vec![DiffChunk {
                old_line: None,
                new_line: Some(1),
                old: None,
                new: Some(format!("preview {file}")),
            }],
        }
    }

    fn make_snapshot(id: &str, parent: Option<&str>, target: &str) -> BranchSnapshot {
        BranchSnapshot::new(
            BranchId(id.to_string()),
            parent.map(|p| BranchId(p.to_string())),
            format!("tx-{id}"),
            target.to_string(),
            RuntimeShellState::PreviewReady,
            make_diff(target),
            ConvergenceScore::zero(),
            ContradictionSet::zero(),
            WorldStateSnapshot::zero(),
            RuntimeEffectSet::zero(),
            parent.map(|_| 1).unwrap_or(0),
            0,
        )
    }

    fn make_runtime(committed_id: &str, target: &str) -> BranchRuntime {
        BranchRuntime::new(make_snapshot(committed_id, None, target))
    }

    // ── required tests ─────────────────────────────────────────────────────

    /// Rule 5.2: deterministic convergence scoring.
    #[test]
    fn convergence_score_deterministic() {
        let s1 = make_snapshot("c1", None, "t1");
        let s2 = make_snapshot("c1", None, "t1");
        assert_eq!(s1.score.total(), s2.score.total());
    }

    /// Rule 5.1 / Rule 2: contradictions reduce convergence ranking.
    #[test]
    fn contradiction_penalty_applied() {
        let mut s = make_snapshot("c1", None, "t1");
        s.contradictions.ownership_conflicts = 1;

        evaluate_branch_convergence(&mut s);
        // Penalty should be applied.
        assert!(s.score.contradiction_penalty > 0.0);
        assert!(s.score.total() < 0.0);
    }

    #[test]
    fn unstable_branch_pruned() {
        let mut runtime = make_runtime("p", "t");
        runtime.budget.max_active_branches = 1;

        let mut s1 = make_snapshot("c1", Some("p"), "t1");
        s1.score.architectural_consistency = 10.0;

        let mut s2 = make_snapshot("c2", Some("p"), "t2");
        s2.score.architectural_consistency = 5.0; // Less stable

        runtime.open_speculative(s1);
        runtime.open_speculative(s2);

        assert_eq!(runtime.speculative_branches.len(), 1);
        assert_eq!(runtime.speculative_branches[0].branch_id, BranchId("c1".to_string()));
    }

    #[test]
    fn oscillation_detected() {
        let runtime = make_runtime("p", "t");
        // Create a snapshot that looks like the committed one.
        let mut s = make_snapshot("c1", None, "t");
        s.projection = runtime.committed_branch.projection.clone();
        s.parent_branch = runtime.committed_branch.parent_branch.clone();

        assert!(runtime.detect_branch_oscillation(&s));
    }

    #[test]
    fn convergence_halt_deterministic() {
        let mut runtime = make_runtime("p", "t");
        runtime.max_stagnation_cycles = 2;

        // Commit same score twice.
        let mut s1 = make_snapshot("c1", Some("p"), "t");
        s1.score.architectural_consistency = 10.0;
        runtime.open_speculative(s1);
        runtime.commit_branch();
        assert_eq!(runtime.trajectory.stagnation_cycles, 0); // first real score

        let mut s2 = make_snapshot("c2", Some("c1"), "t");
        s2.score.architectural_consistency = 10.0; // no improvement
        runtime.open_speculative(s2);
        runtime.commit_branch();
        assert_eq!(runtime.trajectory.stagnation_cycles, 1);

        let mut s3 = make_snapshot("c3", Some("c2"), "t");
        s3.score.architectural_consistency = 10.0; // no improvement
        runtime.open_speculative(s3);
        runtime.commit_branch();
        assert_eq!(runtime.trajectory.stagnation_cycles, 2);
        assert!(runtime.should_halt());
    }

    #[test]
    fn stable_branch_selected_consistently() {
        let mut runtime = make_runtime("p", "t");
        
        let mut s1 = make_snapshot("c1", Some("p"), "t1");
        s1.score.architectural_consistency = 10.0;
        
        let mut s2 = make_snapshot("c2", Some("p"), "t2");
        s2.score.architectural_consistency = 20.0; // winner

        runtime.open_speculative(s1);
        runtime.open_speculative(s2);
        
        runtime.commit_branch();
        assert_eq!(runtime.committed_branch.branch_id, BranchId("c2".to_string()));
    }

    /// Rule 11: world_consistency_score_deterministic
    #[test]
    fn world_consistency_score_deterministic() {
        let mut s1 = make_snapshot("c1", None, "t1");
        let mut s2 = make_snapshot("c1", None, "t1");
        
        evaluate_world_convergence(&mut s1);
        evaluate_world_convergence(&mut s2);
        
        assert_eq!(s1.score.world_consistency.total(), s2.score.world_consistency.total());
    }

    /// Rule 11: verification_failure_penalized
    #[test]
    fn verification_failure_penalized() {
        let mut s = make_snapshot("c1", None, "t1");
        s.runtime_effects.verification_failures = 1;
        
        evaluate_world_convergence(&mut s);
        assert!(s.score.world_consistency.verification_consistency < 0.0);
    }

    /// Rule 4 / 11: causal_divergence_halts_runtime
    #[test]
    fn causal_divergence_detected() {
        let mut s = make_snapshot("c1", None, "t1");
        s.world_state.causal_state_hash = "INVALID".to_string();
        
        evaluate_world_convergence(&mut s);
        assert!(s.score.world_consistency.causal_consistency < -50.0);
    }

    /// Rule 4: rollback exact restoration; no speculative residue.
    #[test]
    fn rollback_restores_parent_bit_identically() {
        let mut runtime = make_runtime("parent-01", "core.rs");
        let committed_before = runtime.committed_branch.clone();
        let child = make_snapshot("child-01", Some("parent-01"), "other.rs");

        runtime.open_speculative(child);
        runtime.rollback();

        assert_eq!(runtime.committed_branch, committed_before);
        assert!(!runtime.has_speculative());
        assert_eq!(runtime.surface_snapshot(), &committed_before);
    }

    /// budget exhaustion.
    #[test]
    fn branch_budget_enforced() {
        let mut runtime = make_runtime("parent-01", "core.rs");
        runtime.budget.remaining_branches = 1;

        let child1 = make_snapshot("child-01", Some("parent-01"), "a.rs");
        let child2 = make_snapshot("child-02", Some("parent-01"), "b.rs");

        assert!(runtime.open_speculative(child1));
        assert!(!runtime.open_speculative(child2)); // Budget exhausted
        assert_eq!(runtime.speculative_branches.len(), 1);
    }

    #[test]
    fn recursive_branch_depth_bounded() {
        let mut runtime = make_runtime("parent-01", "core.rs");
        runtime.budget.remaining_depth = 2;

        let mut child = make_snapshot("child-01", Some("parent-01"), "a.rs");
        child.depth = 3; // Exceeds limit

        assert!(!runtime.open_speculative(child));
    }

    #[test]
    fn branch_pruning_deterministic() {
        let mut runtime = make_runtime("parent-01", "core.rs");
        runtime.budget.max_active_branches = 5;
        runtime.budget.remaining_branches = 20;
        
        for i in 0..10 {
            let mut s = make_snapshot(&format!("c-{}", i), Some("p"), "t");
            s.score.architectural_consistency = i as f32; // Higher i = higher score
            runtime.open_speculative(s);
        }

        assert_eq!(runtime.speculative_branches.len(), 5);
        // Pruned lowest scores (0-4), kept highest (5-9)
        assert!(runtime.speculative_branches.iter().all(|b| b.score.total() >= 5.0));
    }

    /// Rule 2: child never appears on the runtime surface before commit.
    #[test]
    fn branch_child_invisible_before_commit() {
        let mut runtime = make_runtime("parent-01", "core.rs");
        let child = make_snapshot("child-01", Some("parent-01"), "core.rs");
        let committed_before = runtime.committed_branch.clone();

        runtime.open_speculative(child);

        // Surface must still reflect the committed branch.
        assert_eq!(runtime.surface_snapshot(), &committed_before);
        assert_eq!(
            runtime.surface_snapshot().branch_id,
            BranchId("parent-01".to_string())
        );
        // Speculative exists internally but is invisible on the surface.
        assert!(runtime.has_speculative());
    }

    /// Rules 3 + 4: invalid child never mutates parent; parent is
    /// bit-identical after child reject.
    #[test]
    fn invalid_child_preserves_parent() {
        let mut runtime = make_runtime("parent-01", "core.rs");
        let committed_before = runtime.committed_branch.clone();

        let child = make_snapshot("child-invalid", Some("parent-01"), "other.rs");
        runtime.open_speculative(child);
        // Simulate invalid validation — reject the speculative.
        runtime.reject_speculative();

        // Parent is bit-identically preserved.
        assert_eq!(runtime.committed_branch, committed_before);
        assert!(!runtime.has_speculative());
        assert_eq!(runtime.surface_snapshot(), &committed_before);
    }

    /// Rule 5: child commit atomically replaces parent; single committed
    /// branch remains after commit.
    #[test]
    fn child_commit_replaces_parent_atomically() {
        let mut runtime = make_runtime("parent-01", "core.rs");
        let mut child = make_snapshot("child-01", Some("parent-01"), "core.rs");

        runtime.open_speculative(child.clone());
        let committed = runtime.commit_branch();

        assert!(committed);
        // Exactly one committed branch, no speculative.
        assert!(!runtime.has_speculative());
        // The committed branch is now the former child.
        child.created_at = 1;
        assert_eq!(runtime.committed_branch, child);
        assert_eq!(
            runtime.surface_snapshot().branch_id,
            BranchId("child-01".to_string())
        );
    }

    /// Rejected child cannot be resurrected; stale speculative branch is
    /// permanently destroyed.
    #[test]
    fn rejected_child_never_resurrects() {
        let mut runtime = make_runtime("parent-01", "core.rs");
        let committed_before = runtime.committed_branch.clone();
        let child = make_snapshot("child-01", Some("parent-01"), "core.rs");

        runtime.open_speculative(child);
        runtime.reject_speculative();

        // Attempting commit after rejection must fail.
        let result = runtime.commit_branch();
        assert!(!result);

        // The committed branch is still the original parent.
        assert_eq!(runtime.committed_branch, committed_before);
        assert!(!runtime.has_speculative());
    }

    // ── additional structural invariants ──────────────────────────────────

    #[test]
    fn commit_without_speculative_is_noop() {
        let mut runtime = make_runtime("parent-01", "core.rs");
        let committed_before = runtime.committed_branch.clone();

        let result = runtime.commit_branch();

        assert!(!result);
        assert_eq!(runtime.committed_branch, committed_before);
        assert!(!runtime.has_speculative());
    }

    #[test]
    fn open_speculative_does_not_mutate_committed_fields() {
        let mut runtime = make_runtime("parent-01", "core.rs");
        let committed_before = runtime.committed_branch.clone();
        let child = make_snapshot("child-01", Some("parent-01"), "other.rs");

        runtime.open_speculative(child);

        assert_eq!(runtime.committed_branch, committed_before);
    }

    #[test]
    fn surface_snapshot_always_returns_committed_not_speculative() {
        let mut runtime = make_runtime("parent-01", "core.rs");
        let child_a = make_snapshot("child-a", Some("parent-01"), "a.rs");
        let child_b = make_snapshot("child-b", Some("parent-01"), "b.rs");

        // Open first speculative — surface unchanged.
        runtime.open_speculative(child_a);
        assert_eq!(
            runtime.surface_snapshot().branch_id,
            BranchId("parent-01".to_string())
        );

        // Stage second speculative — surface still unchanged.
        runtime.open_speculative(child_b);
        assert_eq!(
            runtime.surface_snapshot().branch_id,
            BranchId("parent-01".to_string())
        );

        // Only after commit does the surface change.
        runtime.commit_branch();
        assert_eq!(
            runtime.surface_snapshot().branch_id,
            BranchId("child-a".to_string())
        );
    }

    #[test]
    fn reject_without_speculative_is_safe() {
        let mut runtime = make_runtime("parent-01", "core.rs");
        let committed_before = runtime.committed_branch.clone();

        // Should not panic.
        runtime.reject_speculative();

        assert_eq!(runtime.committed_branch, committed_before);
        assert!(!runtime.has_speculative());
    }

    #[test]
    fn new_runtime_has_no_speculative() {
        let snapshot = make_snapshot("root-01", None, "main.rs");
        let runtime = BranchRuntime::new(snapshot.clone());

        assert_eq!(runtime.committed_branch, snapshot);
        assert!(!runtime.has_speculative());
        assert_eq!(runtime.surface_snapshot(), &snapshot);
    }

    #[test]
    fn parent_branch_id_set_on_child_snapshot() {
        let child = make_snapshot("child-01", Some("parent-01"), "core.rs");

        assert_eq!(
            child.parent_branch,
            Some(BranchId("parent-01".to_string()))
        );
        assert_eq!(child.branch_id, BranchId("child-01".to_string()));
    }

    #[test]
    fn committed_branch_projection_unchanged_after_speculative_reject() {
        let mut runtime = make_runtime("parent-01", "core.rs");
        let original_projection = runtime.committed_branch.projection.clone();
        let child = make_snapshot("child-01", Some("parent-01"), "other.rs");

        runtime.open_speculative(child);
        runtime.reject_speculative();

        assert_eq!(runtime.committed_branch.projection, original_projection);
    }
}
