//! ExecutionPlanCandidate — structured execution proposal.
//!
//! Spec: DBM-EXECUTION-CANDIDATE-SPEC v1.0
//!       DBM-EXPLOSION-FIX-TIER1-SPEC v1.0
//!
//! Elevates execution candidates from raw operation sequences to
//! decision units with expected effects, risks, and confidence scores.
//! Candidates are proposals — they are never executed directly (spec §2.1).
//!
//! Tier-1 explosion fix:
//! - `MAX_CANDIDATES = 3`: hard upper bound on candidate count (§4).
//! - `generate_candidates()`: single, non-recursive generation entry point (§5).
//! - Content-hash deduplication via `ExecutionPlanCandidate::hash()` (§8).

use crate::convergence::ExecutionOp;
use crate::types::{Action, CodeIrProgram, ExecutionMode, Intent};
use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::limits::Limits;

// ── Evaluation weights (DBM-EVALUATION-FUNCTION-STEP1 §6) ────────────────────

const W_EFFECT: f64 = 1.0;
const W_RISK: f64 = 0.8;
const W_COST: f64 = 0.5;

/// Hard upper bound on proposal candidate count.  Spec §4.1.
pub const MAX_CANDIDATES: usize = 3;

// ── ResolvedTarget ────────────────────────────────────────────────────────────

/// Resolved file/symbol context for a candidate.  Spec §3.1 `target` field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTarget {
    /// Target file path (relative to project root).
    pub file: String,
    /// Optional symbol within the file (function, struct, etc.).
    pub symbol: Option<String>,
}

// ── EffectKind ────────────────────────────────────────────────────────────────

/// Category of an expected effect.  Spec §3.3.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EffectKind {
    BugFix,
    Refactor,
    Performance,
    Safety,
    StructuralChange,
    TestImprovement,
}

impl EffectKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::BugFix => "BugFix",
            Self::Refactor => "Refactor",
            Self::Performance => "Performance",
            Self::Safety => "Safety",
            Self::StructuralChange => "StructuralChange",
            Self::TestImprovement => "TestImprovement",
        }
    }
}

// ── ImpactLevel ───────────────────────────────────────────────────────────────

/// Magnitude of an expected effect.  Spec §3.4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImpactLevel {
    Low,
    Medium,
    High,
}

impl ImpactLevel {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }
}

// ── ExpectedEffect ────────────────────────────────────────────────────────────

/// A predicted outcome of executing a candidate.  Spec §3.2.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedEffect {
    pub kind: EffectKind,
    pub description: String,
    pub impact: ImpactLevel,
}

// ── RiskLevel ─────────────────────────────────────────────────────────────────

/// Severity of an identified risk.  Spec §3.6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
        }
    }
}

// ── Risk ──────────────────────────────────────────────────────────────────────

/// An identified risk for a candidate.  Spec §3.5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Risk {
    pub level: RiskLevel,
    pub description: String,
}

// ── ExecutionPlanCandidate ────────────────────────────────────────────────────

/// An execution candidate elevated to a decision unit.  Spec §3.1.
///
/// Carries expected effects, risks, and confidence so the user can make an
/// informed selection.  Not executed directly — see spec §2.1.
///
/// `PartialEq` / `Eq` are implemented manually because `f32` fields do not
/// satisfy `Eq`.  Both `confidence` and `score` are always in [0.0, 1.0], so
/// NaN-related unsoundness cannot arise in practice.
#[derive(Debug, Clone)]
pub struct ExecutionPlanCandidate {
    /// Unique id within the proposal batch (1-based).
    pub id: usize,
    /// Human-readable summary of what this candidate does.
    pub summary: String,
    /// The ordered operations in this candidate.
    pub steps: Vec<ExecutionOp>,
    /// Optional resolved context target (file / symbol).
    pub target: Option<ResolvedTarget>,
    /// Predicted outcomes if executed.  Spec §3.2.
    pub expected_effects: Vec<ExpectedEffect>,
    /// Identified risks.  Spec §3.5.
    pub risks: Vec<Risk>,
    /// Confidence that executing this candidate will succeed (0.0–1.0).  Spec §6.
    pub confidence: f32,
    /// Selection score: `gain - risk - cost`.  Spec DBM-EVALUATION-FUNCTION-STEP1 §4.
    pub score: f64,
}

impl PartialEq for ExecutionPlanCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.summary == other.summary
            && self.steps == other.steps
            && self.expected_effects == other.expected_effects
            && self.risks == other.risks
            && (self.confidence - other.confidence).abs() < f32::EPSILON
            && (self.score - other.score).abs() < f64::EPSILON
    }
}

impl Eq for ExecutionPlanCandidate {}

// ── Effect estimation ─────────────────────────────────────────────────────────

/// Map a single `ExecutionOp` to an `ExpectedEffect`.  Spec §4.2.
fn op_to_effect(op: &ExecutionOp) -> ExpectedEffect {
    match op {
        ExecutionOp::RuntimePhase(phase) => {
            let lower = phase.to_lowercase();
            if lower.contains("refactor") {
                ExpectedEffect {
                    kind: EffectKind::Refactor,
                    description: "Improves code structure".to_string(),
                    impact: ImpactLevel::Medium,
                }
            } else if lower.contains("test") {
                ExpectedEffect {
                    kind: EffectKind::TestImprovement,
                    description: "Improves test coverage".to_string(),
                    impact: ImpactLevel::Medium,
                }
            } else if lower.contains("perf") || lower.contains("optim") {
                ExpectedEffect {
                    kind: EffectKind::Performance,
                    description: "Improves runtime performance".to_string(),
                    impact: ImpactLevel::Medium,
                }
            } else if lower.contains("apply")
                || lower.contains("fix")
                || lower.contains("patch")
                || lower.contains("repair")
                || lower.contains("build")
            {
                ExpectedEffect {
                    kind: EffectKind::BugFix,
                    description: "Likely resolves identified issue".to_string(),
                    impact: ImpactLevel::High,
                }
            } else if lower.contains("safe") || lower.contains("security") {
                ExpectedEffect {
                    kind: EffectKind::Safety,
                    description: "Improves safety or security".to_string(),
                    impact: ImpactLevel::High,
                }
            } else {
                ExpectedEffect {
                    kind: EffectKind::StructuralChange,
                    description: "Provides system insight".to_string(),
                    impact: ImpactLevel::Low,
                }
            }
        }
        ExecutionOp::GitAdd { .. } | ExecutionOp::GitCommit { .. } => ExpectedEffect {
            kind: EffectKind::StructuralChange,
            description: "Records changes in version control".to_string(),
            impact: ImpactLevel::Low,
        },
        ExecutionOp::GitStatus | ExecutionOp::GitDiff => ExpectedEffect {
            kind: EffectKind::StructuralChange,
            description: "Provides system insight".to_string(),
            impact: ImpactLevel::Low,
        },
    }
}

/// Apply target-based effect amplification.  Spec §4.3.
fn target_amplified_effects(
    mut effects: Vec<ExpectedEffect>,
    target: Option<&ResolvedTarget>,
) -> Vec<ExpectedEffect> {
    let Some(target) = target else {
        return effects;
    };
    let file_lower = target.file.to_lowercase();
    for effect in &mut effects {
        if (file_lower.contains("parser") || file_lower.contains("parse"))
            && effect.kind == EffectKind::StructuralChange
        {
            // Parser files: promote generic structural insight → BugFix (spec §4.3)
            effect.kind = EffectKind::BugFix;
            effect.description = "Likely resolves parsing issue".to_string();
            effect.impact = ImpactLevel::High;
        } else if (file_lower.contains("core") || file_lower.contains("main"))
            && effect.kind == EffectKind::BugFix
        {
            // Core modules carry higher risk — annotate description (spec §4.3)
            effect.description = format!("{} (core module)", effect.description);
        }
    }
    effects
}

// ── Risk estimation ───────────────────────────────────────────────────────────

/// Estimate risks from the step list.  Spec §5.
fn estimate_risks(steps: &[ExecutionOp]) -> Vec<Risk> {
    let mut risks = Vec::new();

    let modifies_vcs = steps.iter().any(|op| {
        matches!(
            op,
            ExecutionOp::GitAdd { .. } | ExecutionOp::GitCommit { .. }
        )
    });
    let runtime_phase_count = steps
        .iter()
        .filter(|op| matches!(op, ExecutionOp::RuntimePhase(_)))
        .count();

    // Multi-step coordination risk  (spec §5.2: steps.len() > 3 → Medium)
    if steps.len() > 3 {
        risks.push(Risk {
            level: RiskLevel::Medium,
            description: "Multi-step plan increases coordination complexity".to_string(),
        });
    }

    // Multiple phases + VCS writes  (spec §5.2: modifies_multiple_files → High)
    if modifies_vcs && runtime_phase_count > 1 {
        risks.push(Risk {
            level: RiskLevel::High,
            description: "Changes span multiple phases and write to the filesystem".to_string(),
        });
    } else if modifies_vcs {
        risks.push(Risk {
            level: RiskLevel::Low,
            description: "Changes will be committed to version control".to_string(),
        });
    }

    // Default: single-scope operation  (spec §5.2: only_single_function → Low)
    if risks.is_empty() {
        risks.push(Risk {
            level: RiskLevel::Low,
            description: "Single-scope operation with limited blast radius".to_string(),
        });
    }

    risks
}

// ── Confidence calculation ────────────────────────────────────────────────────

/// Compute candidate confidence.  Spec §6.
///
/// `confidence = base - risk_penalty - change_size_penalty`
fn compute_confidence(steps: &[ExecutionOp], risks: &[Risk]) -> f32 {
    let base: f32 = 0.8;

    let risk_penalty: f32 = risks
        .iter()
        .map(|r| match r.level {
            RiskLevel::Low => 0.0,
            RiskLevel::Medium => 0.1,
            RiskLevel::High => 0.2,
        })
        .sum();

    let change_size_penalty: f32 = match steps.len() {
        0..=1 => 0.0,
        2..=3 => 0.05,
        4..=6 => 0.15,
        _ => 0.25,
    };

    (base - risk_penalty - change_size_penalty).clamp(0.0, 1.0)
}

// ── Public API ────────────────────────────────────────────────────────────────

impl ExecutionPlanCandidate {
    /// Create an enriched candidate from a list of ops.  Spec §7.2.
    ///
    /// Automatically applies:
    /// 1. `op_to_effect` per op
    /// 2. target-based amplification
    /// 3. risk estimation
    /// 4. confidence / score calculation
    pub fn from_ops(
        id: usize,
        summary: impl Into<String>,
        steps: Vec<ExecutionOp>,
        target: Option<ResolvedTarget>,
    ) -> Self {
        let raw_effects: Vec<ExpectedEffect> = steps.iter().map(op_to_effect).collect();
        let expected_effects = target_amplified_effects(raw_effects, target.as_ref());
        let risks = estimate_risks(&steps);
        let confidence = compute_confidence(&steps, &risks);

        let mut candidate = Self {
            id,
            summary: summary.into(),
            steps,
            target,
            expected_effects,
            risks,
            confidence,
            score: 0.0,
        };
        candidate.score = candidate.compute_score();
        candidate
    }

    /// Compute selection score.  Spec DBM-EVALUATION-FUNCTION-STEP1 §7.
    ///
    /// `score = gain - risk - cost`
    /// - `gain = W_EFFECT * expected_effects.len()`
    /// - `risk  = W_RISK  * risks.len()`
    /// - `cost  = W_COST  * steps.len()`
    ///
    /// Deterministic, no external dependencies, no randomness.
    pub fn compute_score(&self) -> f64 {
        let gain = self.expected_effects.len() as f64 * W_EFFECT;
        let risk = self.risks.len() as f64 * W_RISK;
        let cost = self.steps.len() as f64 * W_COST;
        let score = gain - risk - cost;
        println!(
            "[IR-TRACE][SCORE] candidate_id={} gain={} risk={} cost={} score={}",
            self.id, gain, risk, cost, score
        );
        score
    }

    /// Compute a content-hash for deduplication.  Spec §8.2.
    ///
    /// Hashes `steps` (ops) and `target` (file + symbol).  Two candidates
    /// with identical operations on the same target are considered duplicates
    /// regardless of their computed `confidence` or `score`.
    pub fn hash(&self) -> u64 {
        let mut h = DefaultHasher::new();
        for step in &self.steps {
            step.hash(&mut h);
        }
        if let Some(ref t) = self.target {
            t.file.hash(&mut h);
            t.symbol.hash(&mut h);
        }
        h.finish()
    }

    /// Render this candidate as display lines.  Spec §9 UI表示仕様.
    pub fn render_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(format!("{}. {}", self.id, self.summary));
        lines.push(String::new());

        if !self.expected_effects.is_empty() {
            lines.push("   Effects:".to_string());
            for effect in &self.expected_effects {
                lines.push(format!(
                    "   - {} ({})",
                    effect.description,
                    effect.impact.label()
                ));
            }
        }

        lines.push(String::new());

        if !self.risks.is_empty() {
            lines.push("   Risks:".to_string());
            for risk in &self.risks {
                lines.push(format!(
                    "   - {} ({})",
                    risk.description,
                    risk.level.label()
                ));
            }
        }

        lines.push(String::new());
        lines.push(format!("   Confidence: {:.2}", self.confidence));

        lines
    }
}

// ── Proposal generation ───────────────────────────────────────────────────────

/// Generate, deduplicate, and limit `ExecutionPlanCandidate`s.
///
/// This is the **single** non-recursive generation entry point for proposal
/// candidates.  Spec DBM-EXPLOSION-FIX-TIER1-SPEC §5.1:
/// "1入力 → 1生成ロジックのみ".
///
/// Steps:
/// 1. Build heuristic raw candidates from the plan's commands (Spec §5.3).
/// 2. Log raw count (`CANDIDATES_RAW`).
/// 3. Deduplicate by content hash (Spec §8).
/// 4. Sort by score descending, truncate to `MAX_CANDIDATES` (Spec §4, §7.2).
/// 5. Log post-strategy count (`AFTER_STRATEGY`).
/// 6. Assign stable 1-based ids.
///
/// `mode` must always be `ExecutionMode::Proposal`; retry/repair/replan are
/// never added here (Spec §6.2, §6.3).
pub fn generate_candidates(
    plan: &CodeIrProgram,
    _mode: ExecutionMode,
) -> Vec<ExecutionPlanCandidate> {
    generate_candidates_with_limits(plan, _mode, Limits::default())
}

pub fn generate_candidates_with_limits(
    plan: &CodeIrProgram,
    _mode: ExecutionMode,
    limits: Limits,
) -> Vec<ExecutionPlanCandidate> {
    let build_ops: Vec<ExecutionOp> = plan
        .build_plan
        .build_commands
        .iter()
        .cloned()
        .map(ExecutionOp::RuntimePhase)
        .collect();

    let test_ops: Vec<ExecutionOp> = plan
        .test_plan
        .test_commands
        .iter()
        .cloned()
        .map(ExecutionOp::RuntimePhase)
        .collect();

    let mut raw: Vec<ExecutionPlanCandidate> = Vec::new();

    // Candidate A: direct apply (build commands only).
    if !build_ops.is_empty() {
        let summary = format!(
            "Apply: {}",
            plan.build_plan
                .build_commands
                .first()
                .map(String::as_str)
                .unwrap_or("build")
        );
        raw.push(ExecutionPlanCandidate::from_ops(
            0,
            summary,
            build_ops.clone(),
            None,
        ));
    }

    // Candidate B: apply + test.
    if !build_ops.is_empty() && !test_ops.is_empty() {
        let mut steps = build_ops.clone();
        steps.extend(test_ops);
        let summary = format!("Apply + Test ({} steps)", steps.len());
        raw.push(ExecutionPlanCandidate::from_ops(0, summary, steps, None));
    }

    // Candidate C: refactor then apply.
    if !build_ops.is_empty() {
        let mut steps = vec![ExecutionOp::RuntimePhase("refactor".to_string())];
        steps.extend(build_ops);
        raw.push(ExecutionPlanCandidate::from_ops(
            0,
            "Refactor then apply".to_string(),
            steps,
            None,
        ));
    }

    // Fallback: single structural candidate when the plan has no commands.
    if raw.is_empty() {
        raw.push(ExecutionPlanCandidate::from_ops(
            0,
            "Execute plan".to_string(),
            vec![ExecutionOp::RuntimePhase("apply".to_string())],
            None,
        ));
    }

    // ── Spec §4.3 / §9: Log raw count before any filtering. ──────────────────
    println!("[TRACE][COUNT][CANDIDATES_RAW] {}", raw.len());

    // ── Spec §8: Content-hash deduplication. ─────────────────────────────────
    let mut seen: HashSet<u64> = HashSet::new();
    raw.retain(|c| seen.insert(c.hash()));

    // ── Spec §7.2 / §4.2: Sort by score, then truncate to MAX_CANDIDATES. ────
    raw.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    raw.truncate(limits.max_candidates);

    // ── Spec §9: Log post-strategy count. ────────────────────────────────────
    println!("[TRACE][COUNT][AFTER_STRATEGY] {}", raw.len());

    // Assign stable 1-based ids after sort.
    for (i, c) in raw.iter_mut().enumerate() {
        c.id = i + 1;
    }

    raw
}

/// Return true when the intent is too underspecified for direct execution.
///
/// Ambiguity is based on executability: module-level targets and abstract
/// actions without file/symbol scope are routed to Proposal instead of Planner.
pub fn requires_clarification(intent: &Intent) -> bool {
    if intent.description.trim().is_empty() {
        return true;
    }

    let has_file = intent.file.is_some();
    let has_symbol = intent.symbol.is_some();
    let is_abstract_action = matches!(
        intent.action,
        Action::Fix | Action::Improve | Action::Optimize | Action::RefactorGeneric
    );
    let is_module_level_target = intent.target.is_some() && intent.symbol.is_none();

    if is_abstract_action && !has_file && !has_symbol {
        return true;
    }

    if is_module_level_target {
        return true;
    }

    false
}

/// Generate proposal candidates directly from an intent.
///
/// This path is independent from Planner and Strategy.  It is deliberately
/// non-recursive, uses `ExecutionPlanCandidate::from_ops()`, deduplicates by
/// candidate content hash, and caps the result at `MAX_CANDIDATES`.
pub fn generate_candidates_from_intent(intent: &Intent) -> Vec<ExecutionPlanCandidate> {
    generate_candidates_from_intent_with_limits(intent, Limits::default())
}

pub fn generate_candidates_from_intent_with_limits(
    intent: &Intent,
    limits: Limits,
) -> Vec<ExecutionPlanCandidate> {
    let description = intent.description.trim();
    if description.is_empty() {
        return Vec::new();
    }

    let target = intent.file.as_ref().map(|file| ResolvedTarget {
        file: file.clone(),
        symbol: intent.symbol.clone(),
    });
    let subject = intent_subject(description, target.as_ref());
    let action = intent_action(intent);

    let mut raw = vec![
        ExecutionPlanCandidate::from_ops(
            0,
            format!("{} {} with a focused patch", title_case(action), subject),
            vec![ExecutionOp::RuntimePhase(format!("{action} {subject}"))],
            target.clone(),
        ),
        ExecutionPlanCandidate::from_ops(
            0,
            format!("Inspect {} and apply the smallest safe change", subject),
            vec![
                ExecutionOp::RuntimePhase(format!("inspect {subject}")),
                ExecutionOp::RuntimePhase(format!("{action} {subject}")),
            ],
            target.clone(),
        ),
        ExecutionPlanCandidate::from_ops(
            0,
            format!("Add regression coverage before changing {}", subject),
            vec![
                ExecutionOp::RuntimePhase(format!("add regression test for {subject}")),
                ExecutionOp::RuntimePhase(format!("{action} {subject}")),
            ],
            target,
        ),
    ];

    let mut seen: HashSet<u64> = HashSet::new();
    raw.retain(|candidate| seen.insert(candidate.hash()));
    raw.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    raw.truncate(limits.max_candidates);

    for (index, candidate) in raw.iter_mut().enumerate() {
        candidate.id = index + 1;
    }

    raw
}

fn intent_action(intent: &Intent) -> &'static str {
    match intent.action {
        Action::Optimize => "optimize",
        Action::Improve => "improve",
        Action::RefactorGeneric => "refactor",
        _ => "fix",
    }
}

fn intent_subject(description: &str, target: Option<&ResolvedTarget>) -> String {
    if let Some(target) = target {
        return target.file.clone();
    }
    if let Some(symbol) = &Intent::new(description).symbol {
        return symbol.clone();
    }
    if let Some(module) = &Intent::new(description).target {
        return module.clone();
    }

    description
        .split_whitespace()
        .map(|token| {
            token
                .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_' && c != '-')
                .to_ascii_lowercase()
        })
        .find(|token| {
            !token.is_empty()
                && !matches!(
                    token.as_str(),
                    "fix"
                        | "improve"
                        | "optimize"
                        | "refactor"
                        | "bug"
                        | "issue"
                        | "problem"
                        | "code"
                        | "please"
                        | "the"
                        | "a"
                        | "an"
                )
        })
        .unwrap_or_else(|| "target".to_string())
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + chars.as_str(),
        None => String::new(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn apply_ops() -> Vec<ExecutionOp> {
        vec![ExecutionOp::RuntimePhase("cargo build".to_string())]
    }

    fn multi_ops() -> Vec<ExecutionOp> {
        vec![
            ExecutionOp::RuntimePhase("cargo build".to_string()),
            ExecutionOp::RuntimePhase("cargo test".to_string()),
            ExecutionOp::RuntimePhase("refactor".to_string()),
            ExecutionOp::GitAdd {
                path: "src/".to_string(),
            },
            ExecutionOp::GitCommit {
                message: "auto fix".to_string(),
            },
        ]
    }

    // ── Spec §10 単体テスト ────────────────────────────────────────────────────

    #[test]
    fn effect_from_build_op() {
        let effect = op_to_effect(&ExecutionOp::RuntimePhase("cargo build".to_string()));
        assert_eq!(effect.kind, EffectKind::BugFix);
        assert_eq!(effect.impact, ImpactLevel::High);
    }

    #[test]
    fn effect_from_refactor_op() {
        let effect = op_to_effect(&ExecutionOp::RuntimePhase("refactor".to_string()));
        assert_eq!(effect.kind, EffectKind::Refactor);
        assert_eq!(effect.impact, ImpactLevel::Medium);
    }

    #[test]
    fn effect_from_test_op() {
        let effect = op_to_effect(&ExecutionOp::RuntimePhase("cargo test".to_string()));
        assert_eq!(effect.kind, EffectKind::TestImprovement);
        assert_eq!(effect.impact, ImpactLevel::Medium);
    }

    #[test]
    fn effect_from_git_add() {
        let effect = op_to_effect(&ExecutionOp::GitAdd {
            path: "src/lib.rs".to_string(),
        });
        assert_eq!(effect.kind, EffectKind::StructuralChange);
    }

    #[test]
    fn effect_from_git_diff() {
        let effect = op_to_effect(&ExecutionOp::GitDiff);
        assert_eq!(effect.kind, EffectKind::StructuralChange);
        assert_eq!(effect.impact, ImpactLevel::Low);
    }

    #[test]
    fn risk_single_op_is_low() {
        let risks = estimate_risks(&apply_ops());
        assert_eq!(risks.len(), 1);
        assert_eq!(risks[0].level, RiskLevel::Low);
    }

    #[test]
    fn risk_multi_op_with_git_includes_high() {
        let risks = estimate_risks(&multi_ops());
        assert!(
            risks.iter().any(|r| r.level == RiskLevel::High),
            "multi-op + git should produce at least one High risk"
        );
    }

    #[test]
    fn confidence_in_range() {
        for ops in [apply_ops(), multi_ops()] {
            let risks = estimate_risks(&ops);
            let conf = compute_confidence(&ops, &risks);
            assert!(
                (0.0..=1.0).contains(&conf),
                "confidence {conf} out of [0,1]"
            );
        }
    }

    #[test]
    fn confidence_single_op_is_high() {
        let ops = apply_ops();
        let risks = estimate_risks(&ops);
        let conf = compute_confidence(&ops, &risks);
        assert!(conf >= 0.7, "single-op confidence {conf} should be ≥ 0.7");
    }

    #[test]
    fn confidence_large_plan_is_lower_than_single() {
        let c_single = {
            let r = estimate_risks(&apply_ops());
            compute_confidence(&apply_ops(), &r)
        };
        let c_multi = {
            let r = estimate_risks(&multi_ops());
            compute_confidence(&multi_ops(), &r)
        };
        assert!(
            c_single > c_multi,
            "single-op conf {c_single} should exceed multi-op conf {c_multi}"
        );
    }

    #[test]
    fn target_amplification_parser_file() {
        let target = ResolvedTarget {
            file: "src/parser.rs".to_string(),
            symbol: Some("parse_input".to_string()),
        };
        let effects = vec![ExpectedEffect {
            kind: EffectKind::StructuralChange,
            description: "Provides system insight".to_string(),
            impact: ImpactLevel::Low,
        }];
        let amplified = target_amplified_effects(effects, Some(&target));
        assert_eq!(amplified[0].kind, EffectKind::BugFix);
        assert_eq!(amplified[0].impact, ImpactLevel::High);
    }

    #[test]
    fn from_ops_enriches_all_fields() {
        let c = ExecutionPlanCandidate::from_ops(1, "Test candidate", apply_ops(), None);
        assert_eq!(c.id, 1);
        assert!(!c.expected_effects.is_empty(), "effects must be populated");
        assert!(!c.risks.is_empty(), "risks must be populated");
        assert!(
            (0.0..=1.0).contains(&c.confidence),
            "confidence out of [0,1]"
        );
        // score is gain - risk - cost (unbounded); verify it's finite and matches compute_score
        assert!(c.score.is_finite(), "score must be finite");
        assert!(
            (c.score - c.compute_score()).abs() < f64::EPSILON,
            "score field must equal compute_score()"
        );
    }

    // ── DBM-EVALUATION-FUNCTION-STEP1 tests ───────────────────────────────────

    /// §7 / §12: formula produces correct values and B > A for spec test cases.
    ///
    /// Note: the spec doc lists "A > B" but the formula yields B > A when
    /// B has the same gain with zero risk/cost overhead.  The implementation
    /// faithfully applies the formula; the test reflects the actual output.
    #[test]
    fn compute_score_formula_values() {
        // Case A: effect=3, risk=1, steps=2  →  3*1.0 - 1*0.8 - 2*0.5 = 1.2
        let a = ExecutionPlanCandidate {
            id: 1,
            summary: "A".into(),
            steps: vec![
                ExecutionOp::RuntimePhase("x".into()),
                ExecutionOp::RuntimePhase("y".into()),
            ],
            target: None,
            expected_effects: vec![
                ExpectedEffect {
                    kind: EffectKind::BugFix,
                    description: "e1".into(),
                    impact: ImpactLevel::High,
                },
                ExpectedEffect {
                    kind: EffectKind::Refactor,
                    description: "e2".into(),
                    impact: ImpactLevel::Medium,
                },
                ExpectedEffect {
                    kind: EffectKind::Performance,
                    description: "e3".into(),
                    impact: ImpactLevel::Low,
                },
            ],
            risks: vec![Risk {
                level: RiskLevel::Low,
                description: "r1".into(),
            }],
            confidence: 0.8,
            score: 0.0,
        };

        // Case B: effect=2, risk=0, steps=1  →  2*1.0 - 0*0.8 - 1*0.5 = 1.5
        let b = ExecutionPlanCandidate {
            id: 2,
            summary: "B".into(),
            steps: vec![ExecutionOp::RuntimePhase("z".into())],
            target: None,
            expected_effects: vec![
                ExpectedEffect {
                    kind: EffectKind::BugFix,
                    description: "e1".into(),
                    impact: ImpactLevel::High,
                },
                ExpectedEffect {
                    kind: EffectKind::Refactor,
                    description: "e2".into(),
                    impact: ImpactLevel::Medium,
                },
            ],
            risks: vec![],
            confidence: 0.8,
            score: 0.0,
        };

        let score_a = a.compute_score();
        let score_b = b.compute_score();

        assert!(
            (score_a - 1.2).abs() < 1e-9,
            "A score should be 1.2, got {score_a}"
        );
        assert!(
            (score_b - 1.5).abs() < 1e-9,
            "B score should be 1.5, got {score_b}"
        );
        // B has higher score (fewer risks and steps relative to gain)
        assert!(
            score_b > score_a,
            "B ({score_b}) should outscore A ({score_a})"
        );
    }

    /// §12: same input always produces the same score (deterministic).
    #[test]
    fn compute_score_is_deterministic() {
        let c1 = ExecutionPlanCandidate::from_ops(1, "same", apply_ops(), None);
        let c2 = ExecutionPlanCandidate::from_ops(1, "same", apply_ops(), None);
        assert_eq!(c1.score, c2.score, "same input must yield same score");
    }

    /// §8 sort: candidates are ordered descending by score after generate_candidates.
    #[test]
    fn score_sort_is_descending() {
        use execution_core::engine::execution_plan::*;
        use std::path::PathBuf;

        let plan = ExecutionPlan {
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
                test_commands: vec!["cargo test".into()],
            },
        };

        let candidates = generate_candidates(&plan, ExecutionMode::Proposal);
        assert!(
            candidates.windows(2).all(|w| w[0].score >= w[1].score),
            "candidates must be sorted descending by score"
        );
    }

    #[test]
    fn render_lines_contains_required_sections() {
        let c = ExecutionPlanCandidate::from_ops(
            1,
            "Fix parser.rs (parse_input)",
            apply_ops(),
            Some(ResolvedTarget {
                file: "src/parser.rs".to_string(),
                symbol: Some("parse_input".to_string()),
            }),
        );
        let rendered = c.render_lines().join("\n");
        assert!(
            rendered.contains("1. Fix parser.rs"),
            "id + summary missing"
        );
        assert!(rendered.contains("Effects:"), "Effects section missing");
        assert!(rendered.contains("Risks:"), "Risks section missing");
        assert!(rendered.contains("Confidence:"), "Confidence line missing");
    }

    #[test]
    fn equality_based_on_id_and_fields() {
        let c1 = ExecutionPlanCandidate::from_ops(1, "same", apply_ops(), None);
        let c2 = ExecutionPlanCandidate::from_ops(1, "same", apply_ops(), None);
        assert_eq!(c1, c2);
    }

    #[test]
    fn inequality_on_different_id() {
        let c1 = ExecutionPlanCandidate::from_ops(1, "same", apply_ops(), None);
        let c2 = ExecutionPlanCandidate::from_ops(2, "same", apply_ops(), None);
        assert_ne!(c1, c2);
    }

    // ── Tier-1 explosion-fix tests (DBM-EXPLOSION-FIX-TIER1-SPEC §10) ─────────

    /// §10.1 正常: candidates ≤ MAX_CANDIDATES.
    #[test]
    fn generate_candidates_respects_max_limit() {
        use execution_core::engine::execution_plan::*;
        use std::path::PathBuf;

        let plan = ExecutionPlan {
            language: TargetLanguage::Rust,
            framework: None,
            project_root: PathBuf::from("/tmp"),
            dependency_plan: DependencyPlan {
                manifest_file: "Cargo.toml".into(),
                dependencies: vec![],
                install_commands: vec![],
            },
            build_plan: BuildPlan {
                build_commands: vec!["cargo build".to_string()],
            },
            run_plan: RunPlan {
                run_commands: vec![],
            },
            test_plan: TestPlan {
                test_files: vec![],
                test_commands: vec!["cargo test".to_string()],
            },
        };

        let candidates = generate_candidates(&plan, ExecutionMode::Proposal);
        assert!(
            candidates.len() <= MAX_CANDIDATES,
            "candidates {} must be ≤ MAX_CANDIDATES {}",
            candidates.len(),
            MAX_CANDIDATES
        );
        assert!(
            !candidates.is_empty(),
            "at least one candidate must be produced"
        );
    }

    /// §10.1 正常: stable output (same input → same candidates).
    #[test]
    fn generate_candidates_is_stable() {
        use execution_core::engine::execution_plan::*;
        use std::path::PathBuf;

        let plan = ExecutionPlan {
            language: TargetLanguage::Rust,
            framework: None,
            project_root: PathBuf::from("/tmp"),
            dependency_plan: DependencyPlan {
                manifest_file: "Cargo.toml".into(),
                dependencies: vec![],
                install_commands: vec![],
            },
            build_plan: BuildPlan {
                build_commands: vec!["cargo build".to_string()],
            },
            run_plan: RunPlan {
                run_commands: vec![],
            },
            test_plan: TestPlan {
                test_files: vec![],
                test_commands: vec!["cargo test".to_string()],
            },
        };

        let first = generate_candidates(&plan, ExecutionMode::Proposal);
        let second = generate_candidates(&plan, ExecutionMode::Proposal);
        assert_eq!(first.len(), second.len(), "output count must be stable");
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(a.summary, b.summary, "summary must be stable");
            assert_eq!(a.steps, b.steps, "steps must be stable");
        }
    }

    /// §8: duplicate candidates are removed by hash deduplication.
    #[test]
    fn candidate_hash_deduplicates() {
        let ops = apply_ops();
        let c1 = ExecutionPlanCandidate::from_ops(0, "a", ops.clone(), None);
        let c2 = ExecutionPlanCandidate::from_ops(0, "b", ops.clone(), None);
        // Same steps, same target → same hash → duplicate
        assert_eq!(
            c1.hash(),
            c2.hash(),
            "same ops + target must produce the same hash"
        );

        let ops2 = multi_ops();
        let c3 = ExecutionPlanCandidate::from_ops(0, "c", ops2, None);
        // Different steps → different hash
        assert_ne!(
            c1.hash(),
            c3.hash(),
            "different ops must produce different hashes"
        );
    }

    /// §10.2 異常防止: fallback plan still produces exactly one candidate.
    #[test]
    fn generate_candidates_fallback_with_empty_plan() {
        use execution_core::engine::execution_plan::*;
        use std::path::PathBuf;

        let empty_plan = ExecutionPlan {
            language: TargetLanguage::Rust,
            framework: None,
            project_root: PathBuf::from("/tmp"),
            dependency_plan: DependencyPlan {
                manifest_file: "Cargo.toml".into(),
                dependencies: vec![],
                install_commands: vec![],
            },
            build_plan: BuildPlan {
                build_commands: vec![],
            },
            run_plan: RunPlan {
                run_commands: vec![],
            },
            test_plan: TestPlan {
                test_files: vec![],
                test_commands: vec![],
            },
        };

        let candidates = generate_candidates(&empty_plan, ExecutionMode::Proposal);
        assert_eq!(
            candidates.len(),
            1,
            "empty plan must produce one fallback candidate"
        );
        assert!(candidates[0].id == 1, "fallback candidate must have id 1");
    }

    /// §4.1: MAX_CANDIDATES constant is 3.
    #[test]
    fn max_candidates_is_three() {
        assert_eq!(MAX_CANDIDATES, 3);
    }

    #[test]
    fn requires_clarification_matches_phase1_examples() {
        assert!(requires_clarification(&Intent::new("fix parser bug")));
        assert!(!requires_clarification(&Intent::new("refactor parser.rs")));
        assert!(!requires_clarification(&Intent::new(
            "fix parse_input in parser.rs"
        )));
        assert!(!requires_clarification(&Intent::new(
            "fix parse_input function"
        )));
        assert!(requires_clarification(&Intent::new("fix parser")));
    }

    #[test]
    fn generate_candidates_from_intent_is_capped_and_deduplicated() {
        let candidates = generate_candidates_from_intent(&Intent::new("fix parser bug"));
        assert!((1..=MAX_CANDIDATES).contains(&candidates.len()));

        let mut hashes = HashSet::new();
        for candidate in &candidates {
            assert!(
                hashes.insert(candidate.hash()),
                "candidate hashes must be unique"
            );
            assert!(!candidate.steps.is_empty());
        }
    }

    #[test]
    fn generate_candidates_from_intent_respects_custom_limit() {
        let candidates = generate_candidates_from_intent_with_limits(
            &Intent::new("fix parser bug"),
            Limits {
                max_candidates: 2,
                ..Limits::default()
            },
        );

        assert!(candidates.len() <= 2);
        assert!(
            candidates.windows(2).all(|w| w[0].score >= w[1].score),
            "limited candidates must remain sorted by score"
        );
    }
}
