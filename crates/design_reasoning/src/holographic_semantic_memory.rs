use std::cmp::Ordering;
use std::collections::HashMap;

// ─── Section 3: Holographic Semantic Memory Core ────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct HolographicSemanticMemory {
    pub memory_id: String,
    pub semantic_signature: String,
    pub generalized_abstraction: String,
    pub semantic_roles: Vec<String>,
    pub causal_patterns: Vec<String>,
    pub abstraction_patterns: Vec<String>,
    pub attractor_strength: f64,
    pub uniqueness_score: f64,
    pub lineage: Vec<String>,
}

impl HolographicSemanticMemory {
    pub fn new(
        memory_id: impl Into<String>,
        semantic_signature: impl Into<String>,
        generalized_abstraction: impl Into<String>,
    ) -> Self {
        Self {
            memory_id: memory_id.into(),
            semantic_signature: semantic_signature.into(),
            generalized_abstraction: generalized_abstraction.into(),
            semantic_roles: Vec::new(),
            causal_patterns: Vec::new(),
            abstraction_patterns: Vec::new(),
            attractor_strength: 1.0,
            uniqueness_score: 1.0,
            lineage: Vec::new(),
        }
    }
}

// ─── Section 4: Generalized Abstraction ─────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct GeneralizedAbstraction {
    pub abstraction_id: String,
    pub abstraction_signature: String,
    pub source_memories: Vec<String>,
    pub semantic_stability: f64,
    pub abstraction_depth: f64,
}

impl GeneralizedAbstraction {
    pub fn new(
        abstraction_id: impl Into<String>,
        abstraction_signature: impl Into<String>,
    ) -> Self {
        Self {
            abstraction_id: abstraction_id.into(),
            abstraction_signature: abstraction_signature.into(),
            source_memories: Vec::new(),
            semantic_stability: 1.0,
            abstraction_depth: 1.0,
        }
    }
}

// ─── Section 5: Semantic Identity ───────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticIdentityScore {
    pub semantic_equivalence: f64,
    pub abstraction_equivalence: f64,
    pub intent_equivalence: f64,
    pub causal_equivalence: f64,
    pub convergence_equivalence: f64,
    pub total_identity_score: f64,
}

impl SemanticIdentityScore {
    pub fn compute(a: &HolographicSemanticMemory, b: &HolographicSemanticMemory) -> Self {
        let semantic_equivalence = token_overlap(&a.semantic_signature, &b.semantic_signature);
        let abstraction_equivalence =
            token_overlap(&a.generalized_abstraction, &b.generalized_abstraction);
        let intent_equivalence = list_overlap(&a.semantic_roles, &b.semantic_roles);
        let causal_equivalence = list_overlap(&a.causal_patterns, &b.causal_patterns);
        let convergence_equivalence =
            list_overlap(&a.abstraction_patterns, &b.abstraction_patterns);
        let total_identity_score = (semantic_equivalence * 0.35
            + abstraction_equivalence * 0.25
            + intent_equivalence * 0.2
            + causal_equivalence * 0.1
            + convergence_equivalence * 0.1)
            .clamp(0.0, 1.0);
        Self {
            semantic_equivalence,
            abstraction_equivalence,
            intent_equivalence,
            causal_equivalence,
            convergence_equivalence,
            total_identity_score,
        }
    }

    /// Two memories with total_identity_score >= this threshold are considered semantic duplicates.
    pub const DUPLICATE_THRESHOLD: f64 = 0.85;

    pub fn is_duplicate(&self) -> bool {
        self.total_identity_score >= Self::DUPLICATE_THRESHOLD
    }
}

// ─── Section 6: Strict Uniqueness Governance ────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct UniquenessScore {
    pub semantic_uniqueness: f64,
    pub abstraction_uniqueness: f64,
    pub causal_uniqueness: f64,
    pub lineage_uniqueness: f64,
    pub total_uniqueness: f64,
}

impl UniquenessScore {
    pub fn compute(
        candidate: &HolographicSemanticMemory,
        existing: &[HolographicSemanticMemory],
    ) -> Self {
        if existing.is_empty() {
            return Self {
                semantic_uniqueness: 1.0,
                abstraction_uniqueness: 1.0,
                causal_uniqueness: 1.0,
                lineage_uniqueness: 1.0,
                total_uniqueness: 1.0,
            };
        }

        // Semantic uniqueness: max identity vs any existing memory (lower = more unique)
        let max_semantic_sim = existing
            .iter()
            .map(|e| token_overlap(&candidate.semantic_signature, &e.semantic_signature))
            .fold(0.0_f64, f64::max);

        // Abstraction uniqueness
        let max_abs_sim = existing
            .iter()
            .map(|e| {
                token_overlap(
                    &candidate.generalized_abstraction,
                    &e.generalized_abstraction,
                )
            })
            .fold(0.0_f64, f64::max);

        // Causal uniqueness
        let max_causal_sim = existing
            .iter()
            .map(|e| list_overlap(&candidate.causal_patterns, &e.causal_patterns))
            .fold(0.0_f64, f64::max);

        // Lineage uniqueness: does the candidate share lineage with existing?
        let max_lineage_sim = existing
            .iter()
            .map(|e| {
                if e.lineage.is_empty() && candidate.lineage.is_empty() {
                    0.0
                } else {
                    list_overlap(&candidate.lineage, &e.lineage)
                }
            })
            .fold(0.0_f64, f64::max);

        let semantic_uniqueness = (1.0 - max_semantic_sim).clamp(0.0, 1.0);
        let abstraction_uniqueness = (1.0 - max_abs_sim).clamp(0.0, 1.0);
        let causal_uniqueness = (1.0 - max_causal_sim).clamp(0.0, 1.0);
        let lineage_uniqueness = (1.0 - max_lineage_sim).clamp(0.0, 1.0);
        let total_uniqueness = (semantic_uniqueness * 0.4
            + abstraction_uniqueness * 0.3
            + causal_uniqueness * 0.2
            + lineage_uniqueness * 0.1)
            .clamp(0.0, 1.0);

        Self {
            semantic_uniqueness,
            abstraction_uniqueness,
            causal_uniqueness,
            lineage_uniqueness,
            total_uniqueness,
        }
    }

    /// Memories with total_uniqueness < this are rejected as non-unique.
    pub const REJECT_THRESHOLD: f64 = 0.15;

    pub fn is_rejected(&self) -> bool {
        self.total_uniqueness < Self::REJECT_THRESHOLD
    }
}

// ─── Section 8: Semantic Attractor Memory ───────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticAttractor {
    pub attractor_id: String,
    pub generalized_patterns: Vec<String>,
    pub convergence_frequency: f64,
    pub semantic_stability: f64,
    pub drift_resistance: f64,
}

impl SemanticAttractor {
    pub fn new(attractor_id: impl Into<String>, patterns: Vec<String>) -> Self {
        // Initial stability = freq/(freq+1) with freq=1.0 → 0.5, grows monotonically on reinforce
        Self {
            attractor_id: attractor_id.into(),
            generalized_patterns: patterns,
            convergence_frequency: 1.0,
            semantic_stability: 0.5,
            drift_resistance: 0.8,
        }
    }

    /// Rule 8.3: Reinforce attractor on repeated convergence.
    /// semantic_stability = freq/(freq+1) is strictly monotonically increasing.
    pub fn reinforce(&mut self) {
        self.convergence_frequency += 1.0;
        self.semantic_stability =
            (self.convergence_frequency / (self.convergence_frequency + 1.0)).clamp(0.0, 1.0);
        self.drift_resistance = (self.drift_resistance + 0.05).clamp(0.0, 1.0);
    }

    /// Rule 8.4: Detect attractor drift.
    pub fn detect_drift(&self, new_patterns: &[String]) -> f64 {
        if self.generalized_patterns.is_empty() {
            return 1.0;
        }
        let overlap = list_overlap(&self.generalized_patterns, new_patterns);
        1.0 - overlap
    }

    pub fn is_drifting(&self, new_patterns: &[String]) -> bool {
        let drift = self.detect_drift(new_patterns);
        drift > (1.0 - self.drift_resistance)
    }

    pub fn is_unstable(&self) -> bool {
        self.semantic_stability < 0.3
    }
}

// ─── Section 9: Semantic Memory Lineage ─────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticLineage {
    pub lineage_id: String,
    pub ancestor_memories: Vec<String>,
    pub derived_memories: Vec<String>,
    pub abstraction_evolution: Vec<String>,
}

impl SemanticLineage {
    pub fn new(lineage_id: impl Into<String>) -> Self {
        Self {
            lineage_id: lineage_id.into(),
            ancestor_memories: Vec::new(),
            derived_memories: Vec::new(),
            abstraction_evolution: Vec::new(),
        }
    }

    /// Rule 9.1: Append-only lineage extension.
    pub fn append_derived(&mut self, memory_id: &str, abstraction_label: &str) {
        self.derived_memories.push(memory_id.to_string());
        self.abstraction_evolution
            .push(abstraction_label.to_string());
    }

    pub fn register_ancestor(&mut self, memory_id: &str) {
        if !self.ancestor_memories.contains(&memory_id.to_string()) {
            self.ancestor_memories.push(memory_id.to_string());
        }
    }
}

// ─── Section 4: Semantic Generalization Engine ───────────────────────────────

pub struct SemanticGeneralizationEngine {
    fold_threshold: f64,
}

impl Default for SemanticGeneralizationEngine {
    fn default() -> Self {
        // 0.3 allows "cache repair X Y" families (Jaccard ≈ 0.33) to fold together
        Self {
            fold_threshold: 0.3,
        }
    }
}

impl SemanticGeneralizationEngine {
    pub fn new(fold_threshold: f64) -> Self {
        Self { fold_threshold }
    }

    /// Extract common token patterns across a set of memory signatures.
    pub fn extract_common_patterns<'a>(
        &self,
        memories: &'a [HolographicSemanticMemory],
    ) -> Vec<String> {
        if memories.is_empty() {
            return vec![];
        }
        if memories.len() == 1 {
            return memories[0]
                .semantic_signature
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
        }

        // Count term frequency across all signatures
        let mut freq: HashMap<&str, usize> = HashMap::new();
        for mem in memories {
            let tokens: std::collections::HashSet<&str> =
                mem.semantic_signature.split_whitespace().collect();
            for t in tokens {
                *freq.entry(t).or_insert(0) += 1;
            }
        }
        let majority = (memories.len() + 1) / 2;
        let mut common: Vec<String> = freq
            .into_iter()
            .filter(|(_, count)| *count >= majority)
            .map(|(t, _)| t.to_string())
            .collect();
        common.sort();
        common
    }

    /// Fold a group of semantically similar memories into a GeneralizedAbstraction.
    pub fn fold(
        &self,
        abstraction_id: impl Into<String>,
        memories: &[HolographicSemanticMemory],
    ) -> GeneralizedAbstraction {
        let common = self.extract_common_patterns(memories);
        let signature = common.join(" ");
        let source_ids: Vec<String> = memories.iter().map(|m| m.memory_id.clone()).collect();
        let depth = memories.len() as f64;
        let stability = if depth > 0.0 {
            (1.0 / depth).min(1.0) + 0.5
        } else {
            1.0
        };

        GeneralizedAbstraction {
            abstraction_id: abstraction_id.into(),
            abstraction_signature: signature,
            source_memories: source_ids,
            semantic_stability: stability.clamp(0.0, 1.0),
            abstraction_depth: depth,
        }
    }

    /// Rule 4.4: Identify memories that are fold candidates (similar signature).
    pub fn find_fold_groups<'a>(
        &self,
        memories: &'a [HolographicSemanticMemory],
    ) -> Vec<Vec<&'a HolographicSemanticMemory>> {
        let mut groups: Vec<Vec<&HolographicSemanticMemory>> = Vec::new();
        let mut assigned = vec![false; memories.len()];

        for i in 0..memories.len() {
            if assigned[i] {
                continue;
            }
            let mut group = vec![&memories[i]];
            assigned[i] = true;
            for j in (i + 1)..memories.len() {
                if assigned[j] {
                    continue;
                }
                let sim = token_overlap(
                    &memories[i].semantic_signature,
                    &memories[j].semantic_signature,
                );
                if sim >= self.fold_threshold {
                    group.push(&memories[j]);
                    assigned[j] = true;
                }
            }
            groups.push(group);
        }
        groups
    }

    /// Form a SemanticAttractor from a generalized abstraction.
    pub fn form_attractor(
        &self,
        attractor_id: impl Into<String>,
        abstraction: &GeneralizedAbstraction,
    ) -> SemanticAttractor {
        let patterns: Vec<String> = abstraction
            .abstraction_signature
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        SemanticAttractor::new(attractor_id, patterns)
    }
}

// ─── Section 6: Uniqueness Governance Engine ─────────────────────────────────

pub struct UniquenessGovernanceEngine;

impl UniquenessGovernanceEngine {
    /// Rule 12.2: Sort memories by uniqueness ordering.
    pub fn sort_memories(memories: &mut [HolographicSemanticMemory]) {
        memories.sort_by(|a, b| {
            // uniqueness desc
            let u_cmp = b
                .uniqueness_score
                .partial_cmp(&a.uniqueness_score)
                .unwrap_or(Ordering::Equal);
            if u_cmp != Ordering::Equal {
                return u_cmp;
            }
            // attractor_strength desc
            let s_cmp = b
                .attractor_strength
                .partial_cmp(&a.attractor_strength)
                .unwrap_or(Ordering::Equal);
            if s_cmp != Ordering::Equal {
                return s_cmp;
            }
            // memory_id asc
            a.memory_id.cmp(&b.memory_id)
        });
    }

    /// Rule 6.3: Reject or merge duplicate semantic memories.
    /// Returns (accepted, rejected_ids).
    pub fn apply_governance(
        memories: Vec<HolographicSemanticMemory>,
    ) -> (Vec<HolographicSemanticMemory>, Vec<String>) {
        let mut accepted: Vec<HolographicSemanticMemory> = Vec::new();
        let mut rejected: Vec<String> = Vec::new();

        for candidate in memories {
            let id_score = SemanticIdentityScore::compute;
            let is_dup = accepted
                .iter()
                .any(|existing| id_score(&candidate, existing).is_duplicate());

            if is_dup {
                rejected.push(candidate.memory_id.clone());
            } else {
                let uniqueness = UniquenessScore::compute(&candidate, &accepted);
                if uniqueness.is_rejected() {
                    rejected.push(candidate.memory_id.clone());
                } else {
                    let mut m = candidate;
                    m.uniqueness_score = uniqueness.total_uniqueness;
                    accepted.push(m);
                }
            }
        }

        Self::sort_memories(&mut accepted);
        (accepted, rejected)
    }
}

// ─── Section 7: Duplicate Elimination ───────────────────────────────────────

pub struct DuplicateEliminationEngine;

impl DuplicateEliminationEngine {
    /// Rule 7.3: Merge two duplicate memories preserving meaning (Rule 7.4).
    pub fn merge(
        primary: HolographicSemanticMemory,
        duplicate: HolographicSemanticMemory,
    ) -> HolographicSemanticMemory {
        // Merge roles (union, deduplicated)
        let mut roles = primary.semantic_roles.clone();
        for r in &duplicate.semantic_roles {
            if !roles.contains(r) {
                roles.push(r.clone());
            }
        }
        // Merge causal patterns
        let mut causal = primary.causal_patterns.clone();
        for c in &duplicate.causal_patterns {
            if !causal.contains(c) {
                causal.push(c.clone());
            }
        }
        // Merge abstraction patterns
        let mut abs_patterns = primary.abstraction_patterns.clone();
        for a in &duplicate.abstraction_patterns {
            if !abs_patterns.contains(a) {
                abs_patterns.push(a.clone());
            }
        }
        // Lineage unification: append duplicate id to primary lineage
        let mut lineage = primary.lineage.clone();
        lineage.push(duplicate.memory_id.clone());
        for l in &duplicate.lineage {
            if !lineage.contains(l) {
                lineage.push(l.clone());
            }
        }
        // Stronger attractor wins or averages
        let attractor_strength = (primary.attractor_strength + duplicate.attractor_strength) / 2.0;

        HolographicSemanticMemory {
            memory_id: primary.memory_id,
            semantic_signature: primary.semantic_signature,
            generalized_abstraction: primary.generalized_abstraction,
            semantic_roles: roles,
            causal_patterns: causal,
            abstraction_patterns: abs_patterns,
            attractor_strength,
            uniqueness_score: primary.uniqueness_score,
            lineage,
        }
    }

    /// Eliminate all duplicates from a memory slice.
    /// Returns (deduplicated, eliminated_ids).
    pub fn eliminate(
        memories: Vec<HolographicSemanticMemory>,
    ) -> (Vec<HolographicSemanticMemory>, Vec<String>) {
        let mut result: Vec<HolographicSemanticMemory> = Vec::new();
        let mut eliminated: Vec<String> = Vec::new();

        for candidate in memories {
            let dup_idx = result.iter().position(|existing| {
                SemanticIdentityScore::compute(&candidate, existing).is_duplicate()
            });
            if let Some(idx) = dup_idx {
                let existing = result.remove(idx);
                let merged = Self::merge(existing, candidate.clone());
                eliminated.push(candidate.memory_id);
                result.insert(idx, merged);
            } else {
                result.push(candidate);
            }
        }
        (result, eliminated)
    }
}

// ─── Holographic Memory Store ─────────────────────────────────────────────

/// Runtime store for holographic semantic memories.
pub struct HolographicMemoryStore {
    memories: Vec<HolographicSemanticMemory>,
    attractors: Vec<SemanticAttractor>,
    lineages: Vec<SemanticLineage>,
    generalization_engine: SemanticGeneralizationEngine,
    /// Governance event log (Rule 13.2)
    pub governance_events: Vec<GovernanceEvent>,
}

#[derive(Debug, Clone)]
pub enum GovernanceEvent {
    DuplicateRejected {
        memory_id: String,
    },
    DuplicateMerged {
        primary_id: String,
        eliminated_id: String,
    },
    UniquenessRejected {
        memory_id: String,
    },
    AbstractionFolded {
        abstraction_id: String,
        source_count: usize,
    },
    AttractorFormed {
        attractor_id: String,
    },
    AttractorReinforced {
        attractor_id: String,
        new_frequency: f64,
    },
    AttractorDriftDetected {
        attractor_id: String,
        drift: f64,
    },
    MemoryLineageExpanded {
        lineage_id: String,
        memory_id: String,
    },
}

impl Default for HolographicMemoryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl HolographicMemoryStore {
    pub fn new() -> Self {
        Self {
            memories: Vec::new(),
            attractors: Vec::new(),
            lineages: Vec::new(),
            generalization_engine: SemanticGeneralizationEngine::default(),
            governance_events: Vec::new(),
        }
    }

    /// Insert a memory through governance (Rule 10.1).
    /// Returns Ok(()) if accepted, Err(reason) if rejected.
    pub fn insert(&mut self, mut memory: HolographicSemanticMemory) -> Result<(), String> {
        // 1. Semantic identity check vs existing memories
        for existing in &self.memories {
            let id_score = SemanticIdentityScore::compute(&memory, existing);
            if id_score.is_duplicate() {
                self.governance_events
                    .push(GovernanceEvent::DuplicateRejected {
                        memory_id: memory.memory_id.clone(),
                    });
                return Err(format!(
                    "Duplicate of memory '{}' (identity_score={:.3})",
                    existing.memory_id, id_score.total_identity_score
                ));
            }
        }

        // 2. Uniqueness check
        let uniqueness = UniquenessScore::compute(&memory, &self.memories);
        if uniqueness.is_rejected() {
            self.governance_events
                .push(GovernanceEvent::UniquenessRejected {
                    memory_id: memory.memory_id.clone(),
                });
            return Err(format!(
                "Uniqueness too low ({:.3}) for memory '{}'",
                uniqueness.total_uniqueness, memory.memory_id
            ));
        }
        memory.uniqueness_score = uniqueness.total_uniqueness;

        // 3. Check for attractor reinforcement
        for attractor in &mut self.attractors {
            let candidate_patterns: Vec<String> = memory
                .semantic_signature
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();
            if !attractor.is_drifting(&candidate_patterns) {
                attractor.reinforce();
                memory.attractor_strength = attractor.semantic_stability;
                self.governance_events
                    .push(GovernanceEvent::AttractorReinforced {
                        attractor_id: attractor.attractor_id.clone(),
                        new_frequency: attractor.convergence_frequency,
                    });
            }
        }

        self.memories.push(memory);
        UniquenessGovernanceEngine::sort_memories(&mut self.memories);
        Ok(())
    }

    /// Trigger generalization / folding pass (Rule 4.4).
    pub fn run_generalization_pass(&mut self) {
        let groups = self.generalization_engine.find_fold_groups(&self.memories);
        for (i, group) in groups.iter().enumerate() {
            if group.len() < 2 {
                continue;
            }
            let memories_owned: Vec<HolographicSemanticMemory> =
                group.iter().map(|m| (*m).clone()).collect();
            let abstraction_id = format!("ABS_{}", i);
            let abstraction = self
                .generalization_engine
                .fold(&abstraction_id, &memories_owned);
            let source_count = abstraction.source_memories.len();

            // Form attractor from this abstraction
            let attractor_id = format!("ATTR_{}", i);
            let existing = self
                .attractors
                .iter()
                .any(|a| a.attractor_id == attractor_id);
            if !existing {
                let attractor = self
                    .generalization_engine
                    .form_attractor(&attractor_id, &abstraction);
                self.attractors.push(attractor);
                self.governance_events
                    .push(GovernanceEvent::AttractorFormed {
                        attractor_id: attractor_id.clone(),
                    });
            }

            self.governance_events
                .push(GovernanceEvent::AbstractionFolded {
                    abstraction_id,
                    source_count,
                });
        }
    }

    /// Run duplicate elimination pass (Section 7).
    pub fn run_duplicate_elimination(&mut self) {
        let memories = std::mem::take(&mut self.memories);
        let (deduplicated, eliminated) = DuplicateEliminationEngine::eliminate(memories);
        for eid in eliminated {
            // Find which primary absorbed it via lineage
            let primary_id = deduplicated
                .iter()
                .find(|m| m.lineage.contains(&eid))
                .map(|m| m.memory_id.clone())
                .unwrap_or_default();
            self.governance_events
                .push(GovernanceEvent::DuplicateMerged {
                    primary_id,
                    eliminated_id: eid,
                });
        }
        self.memories = deduplicated;
        UniquenessGovernanceEngine::sort_memories(&mut self.memories);
    }

    /// Rule 8.4: Detect attractor drift for all attractors.
    pub fn detect_attractor_drifts(&mut self, new_patterns: &[String]) {
        for attractor in &self.attractors {
            let drift = attractor.detect_drift(new_patterns);
            if attractor.is_drifting(new_patterns) {
                self.governance_events
                    .push(GovernanceEvent::AttractorDriftDetected {
                        attractor_id: attractor.attractor_id.clone(),
                        drift,
                    });
            }
        }
    }

    /// Append to a lineage (Rule 9.1 — append only).
    pub fn expand_lineage(&mut self, lineage_id: &str, memory_id: &str, abstraction_label: &str) {
        if let Some(lineage) = self
            .lineages
            .iter_mut()
            .find(|l| l.lineage_id == lineage_id)
        {
            lineage.append_derived(memory_id, abstraction_label);
            self.governance_events
                .push(GovernanceEvent::MemoryLineageExpanded {
                    lineage_id: lineage_id.to_string(),
                    memory_id: memory_id.to_string(),
                });
        }
    }

    pub fn memories(&self) -> &[HolographicSemanticMemory] {
        &self.memories
    }

    pub fn attractors(&self) -> &[SemanticAttractor] {
        &self.attractors
    }

    pub fn lineages(&self) -> &[SemanticLineage] {
        &self.lineages
    }

    pub fn add_lineage(&mut self, lineage: SemanticLineage) {
        self.lineages.push(lineage);
    }
}

// ─── Utility: Token Similarity ───────────────────────────────────────────────

fn token_overlap(a: &str, b: &str) -> f64 {
    let ta: std::collections::HashSet<&str> = a.split_whitespace().collect();
    let tb: std::collections::HashSet<&str> = b.split_whitespace().collect();
    if ta.is_empty() && tb.is_empty() {
        return 1.0;
    }
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let intersection = ta.intersection(&tb).count() as f64;
    let union = ta.union(&tb).count() as f64;
    intersection / union
}

fn list_overlap(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let sa: std::collections::HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let sb: std::collections::HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let intersection = sa.intersection(&sb).count() as f64;
    let union = sa.union(&sb).count() as f64;
    intersection / union
}

// ─── Section 14: Required Tests ──────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_memory(id: &str, signature: &str, abstraction: &str) -> HolographicSemanticMemory {
        HolographicSemanticMemory::new(id, signature, abstraction)
    }

    // ─── 14.1 Generalization Tests ────────────────────────────────────────

    #[test]
    fn semantic_generalization_deterministic() {
        let engine = SemanticGeneralizationEngine::default();
        let memories = vec![
            make_memory("M1", "cache repair latency reduce", "repair"),
            make_memory("M2", "cache repair throughput improve", "repair"),
        ];
        let abs1 = engine.fold("ABS_0", &memories);
        let abs2 = engine.fold("ABS_0", &memories);
        assert_eq!(abs1, abs2, "Generalization must be deterministic");
    }

    #[test]
    fn abstraction_folding_stable() {
        let engine = SemanticGeneralizationEngine::new(0.5);
        let memories = vec![
            make_memory("M1", "cache repair latency", "repair_pattern"),
            make_memory("M2", "cache repair throughput", "repair_pattern"),
            make_memory("M3", "cache repair availability", "repair_pattern"),
        ];
        let abstraction = engine.fold("ABS_REPAIR", &memories);
        assert_eq!(abstraction.source_memories.len(), 3);
        assert!(
            abstraction.abstraction_signature.contains("cache"),
            "Common token 'cache' must appear"
        );
        assert!(
            abstraction.abstraction_signature.contains("repair"),
            "Common token 'repair' must appear"
        );
        // Stability is computable and within range
        assert!(abstraction.semantic_stability >= 0.0 && abstraction.semantic_stability <= 1.0);
    }

    #[test]
    fn generalized_attractor_formed() {
        let engine = SemanticGeneralizationEngine::default();
        let memories = vec![
            make_memory("M1", "cache repair latency", "repair"),
            make_memory("M2", "cache repair throughput", "repair"),
        ];
        let abstraction = engine.fold("ABS_0", &memories);
        let attractor = engine.form_attractor("ATTR_0", &abstraction);
        assert!(!attractor.generalized_patterns.is_empty());
        assert!(attractor.semantic_stability >= 0.0 && attractor.semantic_stability <= 1.0);
    }

    // ─── 14.2 Uniqueness Tests ────────────────────────────────────────────

    #[test]
    fn semantic_identity_stable() {
        let a = make_memory("M1", "cache repair latency reduce", "repair");
        let b = make_memory("M2", "cache repair latency reduce", "repair");
        let score1 = SemanticIdentityScore::compute(&a, &b);
        let score2 = SemanticIdentityScore::compute(&a, &b);
        assert_eq!(score1, score2, "Identity score must be deterministic");
        assert!(
            score1.is_duplicate(),
            "Identical memories must be flagged as duplicates"
        );
    }

    #[test]
    fn uniqueness_ordering_deterministic() {
        let mut memories = vec![
            {
                let mut m = make_memory("M_B", "beta token", "abs_b");
                m.uniqueness_score = 0.6;
                m.attractor_strength = 0.5;
                m
            },
            {
                let mut m = make_memory("M_A", "alpha token", "abs_a");
                m.uniqueness_score = 0.9;
                m.attractor_strength = 0.8;
                m
            },
            {
                let mut m = make_memory("M_C", "gamma token", "abs_c");
                m.uniqueness_score = 0.9;
                m.attractor_strength = 0.9;
                m
            },
        ];
        UniquenessGovernanceEngine::sort_memories(&mut memories);
        // uniqueness desc -> attractor_strength desc -> id asc
        assert_eq!(memories[0].memory_id, "M_C");
        assert_eq!(memories[1].memory_id, "M_A");
        assert_eq!(memories[2].memory_id, "M_B");

        // Sort again: must be stable (deterministic)
        let ids1: Vec<_> = memories.iter().map(|m| m.memory_id.clone()).collect();
        UniquenessGovernanceEngine::sort_memories(&mut memories);
        let ids2: Vec<_> = memories.iter().map(|m| m.memory_id.clone()).collect();
        assert_eq!(ids1, ids2);
    }

    #[test]
    fn duplicate_detection_stable() {
        let a = make_memory("M1", "cache repair throughput latency", "repair");
        let b = make_memory("M2", "cache repair throughput latency", "repair");
        let score = SemanticIdentityScore::compute(&a, &b);
        assert!(score.is_duplicate());

        // Non-duplicate
        let c = make_memory("M3", "database write consistency quorum", "consistency");
        let score2 = SemanticIdentityScore::compute(&a, &c);
        assert!(!score2.is_duplicate());
    }

    // ─── 14.3 Duplicate Elimination Tests ────────────────────────────────

    #[test]
    fn duplicate_memories_eliminated() {
        let memories = vec![
            make_memory("M1", "cache repair latency reduce", "repair"),
            make_memory("M2", "cache repair latency reduce", "repair"), // duplicate
            make_memory("M3", "database write consistency", "consistency"),
        ];
        let (deduped, eliminated) = DuplicateEliminationEngine::eliminate(memories);
        assert_eq!(deduped.len(), 2, "Duplicate must be eliminated");
        assert_eq!(eliminated.len(), 1, "One eliminated entry");
        assert!(
            deduped
                .iter()
                .any(|m| m.memory_id == "M1" || m.memory_id == "M2"),
            "Primary must survive"
        );
    }

    #[test]
    fn meaning_preserved_after_merge() {
        let mut a = make_memory("M1", "cache repair latency", "repair");
        a.semantic_roles = vec!["CacheCoordinator".to_string()];
        a.causal_patterns = vec!["latency_spike_triggers_repair".to_string()];

        let mut b = make_memory("M2", "cache repair latency", "repair");
        b.semantic_roles = vec!["ThroughputOptimizer".to_string()];
        b.causal_patterns = vec!["throughput_drop_triggers_rebalance".to_string()];

        let merged = DuplicateEliminationEngine::merge(a, b);
        assert!(merged
            .semantic_roles
            .contains(&"CacheCoordinator".to_string()));
        assert!(merged
            .semantic_roles
            .contains(&"ThroughputOptimizer".to_string()));
        assert!(
            merged.causal_patterns.len() == 2,
            "Both causal patterns preserved"
        );
        // Lineage contains eliminated memory id
        assert!(merged.lineage.contains(&"M2".to_string()));
    }

    #[test]
    fn replay_contamination_prevented() {
        // Two memories from the same planning lineage should be recognized as duplicates
        let mut a = make_memory("M1", "plan cache repair latency", "repair_plan");
        a.lineage = vec!["LINEAGE_1".to_string()];
        let mut b = make_memory("M2", "plan cache repair latency", "repair_plan");
        b.lineage = vec!["LINEAGE_1".to_string()];

        let score = SemanticIdentityScore::compute(&a, &b);
        assert!(
            score.is_duplicate(),
            "Replay-equivalent memories must be detected"
        );
    }

    // ─── 14.4 Attractor Tests ────────────────────────────────────────────

    #[test]
    fn semantic_attractor_strengthened() {
        let mut attractor =
            SemanticAttractor::new("ATTR_1", vec!["cache".to_string(), "repair".to_string()]);
        let initial_stability = attractor.semantic_stability;
        attractor.reinforce();
        attractor.reinforce();
        assert!(
            attractor.semantic_stability >= initial_stability,
            "Stability must not decrease after reinforcement"
        );
        assert_eq!(attractor.convergence_frequency, 3.0);
    }

    #[test]
    fn attractor_drift_detected() {
        let attractor =
            SemanticAttractor::new("ATTR_1", vec!["cache".to_string(), "repair".to_string()]);
        // Completely unrelated patterns => high drift
        let drifted_patterns = vec![
            "database".to_string(),
            "write".to_string(),
            "quorum".to_string(),
        ];
        let drift = attractor.detect_drift(&drifted_patterns);
        assert!(drift > 0.5, "Drift must be high for unrelated patterns");
        assert!(attractor.is_drifting(&drifted_patterns));
    }

    #[test]
    fn unstable_attractor_rejected() {
        let mut attractor = SemanticAttractor::new("ATTR_UNSTABLE", vec!["pattern_a".to_string()]);
        attractor.semantic_stability = 0.2; // force unstable
        assert!(attractor.is_unstable());
    }

    // ─── 14.5 Lineage Tests ───────────────────────────────────────────────

    #[test]
    fn append_only_lineage_preserved() {
        let mut lineage = SemanticLineage::new("L1");
        lineage.register_ancestor("M0");
        lineage.append_derived("M1", "repair_abstraction_v1");
        lineage.append_derived("M2", "repair_abstraction_v2");

        assert_eq!(lineage.ancestor_memories, vec!["M0".to_string()]);
        assert_eq!(lineage.derived_memories.len(), 2);
        assert_eq!(lineage.abstraction_evolution.len(), 2);

        // Appending again must not overwrite
        lineage.append_derived("M3", "repair_abstraction_v3");
        assert_eq!(lineage.derived_memories.len(), 3);
        // Previous entries must be intact
        assert_eq!(lineage.derived_memories[0], "M1");
        assert_eq!(lineage.derived_memories[1], "M2");
    }

    #[test]
    fn semantic_lineage_replayable() {
        let mut lineage = SemanticLineage::new("L1");
        lineage.register_ancestor("ROOT");
        lineage.append_derived("M1", "abs_v1");
        lineage.append_derived("M2", "abs_v2");

        // Replay: rebuild identical lineage and verify equality
        let mut replay = SemanticLineage::new("L1");
        replay.register_ancestor("ROOT");
        replay.append_derived("M1", "abs_v1");
        replay.append_derived("M2", "abs_v2");

        assert_eq!(lineage, replay, "Lineage replay must be deterministic");
    }

    // ─── Integration: HolographicMemoryStore ─────────────────────────────

    #[test]
    fn store_rejects_duplicate_on_insert() {
        let mut store = HolographicMemoryStore::new();
        let m1 = make_memory("M1", "cache repair latency reduce", "repair");
        let m2 = make_memory("M2", "cache repair latency reduce", "repair");
        store.insert(m1).expect("First insert must succeed");
        let result = store.insert(m2);
        assert!(result.is_err(), "Duplicate insert must be rejected");
        assert_eq!(store.memories().len(), 1);
        assert!(store
            .governance_events
            .iter()
            .any(|e| matches!(e, GovernanceEvent::DuplicateRejected { .. })));
    }

    #[test]
    fn store_runs_generalization_pass() {
        let mut store = HolographicMemoryStore::new();
        store
            .insert(make_memory("M1", "cache repair latency", "repair"))
            .unwrap();
        store
            .insert(make_memory("M2", "cache repair availability", "repair"))
            .unwrap();
        store.run_generalization_pass();
        assert!(
            store.attractors().len() >= 1,
            "At least one attractor must form"
        );
        assert!(store
            .governance_events
            .iter()
            .any(|e| matches!(e, GovernanceEvent::AttractorFormed { .. })));
    }

    #[test]
    fn store_detects_attractor_drift() {
        let mut store = HolographicMemoryStore::new();
        store
            .insert(make_memory("M1", "cache repair latency", "repair"))
            .unwrap();
        store
            .insert(make_memory("M2", "cache repair availability", "repair"))
            .unwrap();
        store.run_generalization_pass();
        let drifted = vec!["quantum".to_string(), "blockchain".to_string()];
        store.detect_attractor_drifts(&drifted);
        assert!(store
            .governance_events
            .iter()
            .any(|e| matches!(e, GovernanceEvent::AttractorDriftDetected { .. })));
    }

    #[test]
    fn governance_apply_filters_duplicates() {
        let memories = vec![
            make_memory("M1", "cache repair latency reduce", "repair"),
            make_memory("M2", "cache repair latency reduce", "repair"),
            make_memory("M3", "database write consistency quorum", "consistency"),
        ];
        let (accepted, rejected) = UniquenessGovernanceEngine::apply_governance(memories);
        assert_eq!(accepted.len(), 2);
        assert_eq!(rejected.len(), 1);
    }

    // ─── Verification Scenarios (Section 15) ─────────────────────────────

    #[test]
    fn verification_a_repeated_repairs_form_attractor() {
        // Verification A: Multiple same-lineage repairs → attractor formed
        let mut store = HolographicMemoryStore::new();
        store
            .insert(make_memory("R1", "cache repair latency spike", "repair"))
            .unwrap();
        store
            .insert(make_memory("R2", "cache repair throughput drop", "repair"))
            .unwrap();
        store
            .insert(make_memory(
                "R3",
                "cache repair availability loss",
                "repair",
            ))
            .unwrap();
        store.run_generalization_pass();
        assert!(
            store
                .attractors()
                .iter()
                .any(|a| a.generalized_patterns.contains(&"cache".to_string())),
            "Repair attractor with 'cache' pattern must form"
        );
    }

    #[test]
    fn verification_b_duplicate_planning_eliminated() {
        // Verification B: Duplicate planning convergence eliminated
        let memories = vec![
            make_memory("P1", "plan cache shard distribute", "shard_plan"),
            make_memory("P2", "plan cache shard distribute", "shard_plan"),
            make_memory("P3", "plan cache shard distribute", "shard_plan"),
        ];
        let (accepted, eliminated) = DuplicateEliminationEngine::eliminate(memories);
        assert_eq!(accepted.len(), 1, "All duplicates must be collapsed to one");
        assert_eq!(eliminated.len(), 2);
    }

    #[test]
    fn verification_c_abstraction_folding_occurs() {
        // Verification C: Similar abstractions folded
        let engine = SemanticGeneralizationEngine::new(0.5);
        let memories = vec![
            make_memory("M1", "cache shard repair planning", "repair_planning"),
            make_memory("M2", "cache shard repair execution", "repair_execution"),
        ];
        let groups = engine.find_fold_groups(&memories);
        assert_eq!(groups.len(), 1, "Similar memories must group for folding");
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn verification_d_drifted_attractor_detected() {
        // Verification D: Semantic drift injected → AttractorDrift detected
        let mut store = HolographicMemoryStore::new();
        store
            .insert(make_memory("M1", "cache repair latency", "repair"))
            .unwrap();
        store
            .insert(make_memory("M2", "cache repair throughput", "repair"))
            .unwrap();
        store.run_generalization_pass();
        // Inject drift
        let drift_patterns = vec![
            "blockchain".to_string(),
            "consensus".to_string(),
            "immutability".to_string(),
        ];
        store.detect_attractor_drifts(&drift_patterns);
        let has_drift_event = store
            .governance_events
            .iter()
            .any(|e| matches!(e, GovernanceEvent::AttractorDriftDetected { .. }));
        assert!(
            has_drift_event,
            "AttractorDrift must be detected and published"
        );
    }
}
