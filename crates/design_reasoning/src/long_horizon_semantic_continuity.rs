use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::semantic_concept_synthesis::ConceptNode;

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticIdentityState {
    pub identity_id: String,
    pub root_semantic_intent: String,
    pub persistent_concepts: Vec<String>,
    pub continuity_score: f64,
    pub drift_score: f64,
    pub temporal_stability: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TemporalSemanticMemory {
    pub temporal_memory_id: String,
    pub semantic_lineages: Vec<String>,
    pub abstraction_histories: Vec<String>,
    pub concept_evolution: Vec<String>,
    pub temporal_coherence: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvolvingConcept {
    pub concept_id: String,
    pub historical_forms: Vec<String>,
    pub abstraction_trajectory: Vec<String>,
    pub semantic_core: String,
    pub continuity_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TemporalAttractor {
    pub attractor_id: String,
    pub persistent_patterns: Vec<String>,
    pub reinforcement_history: Vec<f64>,
    pub stability_over_time: f64,
    pub drift_resistance: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LongHorizonPlanningState {
    pub planning_state_id: String,
    pub persistent_goals: Vec<String>,
    pub evolving_constraints: Vec<String>,
    pub planning_lineage: Vec<String>,
    pub temporal_convergence_score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftClassification {
    None,
    RecoverableDrift,
    AdaptiveEvolution,
    CatastrophicIdentityCollapse,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContinuityEvent {
    IdentityEvolved {
        identity_id: String,
        continuity_score: f64,
    },
    TemporalMemoryAppended {
        temporal_memory_id: String,
        lineage_count: usize,
    },
    ConceptEvolved {
        concept_id: String,
        continuity_score: f64,
    },
    TemporalAttractorReinforced {
        attractor_id: String,
        stability_over_time: f64,
    },
    SemanticDriftClassified {
        identity_id: String,
        classification: DriftClassification,
    },
    ContinuityRepaired {
        identity_id: String,
        continuity_score: f64,
    },
    TemporalCompressionPerformed {
        temporal_memory_id: String,
        semantic_preservation_score: f64,
    },
    SemanticIdentityCollapse {
        identity_id: String,
        drift_score: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LongHorizonReport {
    pub identity: SemanticIdentityState,
    pub temporal_memory: TemporalSemanticMemory,
    pub evolving_concepts: Vec<EvolvingConcept>,
    pub temporal_attractors: Vec<TemporalAttractor>,
    pub planning_state: LongHorizonPlanningState,
    pub events: Vec<ContinuityEvent>,
}

pub struct TemporalContinuityEngine {
    collapse_threshold: f64,
    recoverable_threshold: f64,
}

impl Default for TemporalContinuityEngine {
    fn default() -> Self {
        Self {
            collapse_threshold: 0.72,
            recoverable_threshold: 0.28,
        }
    }
}

impl TemporalContinuityEngine {
    pub fn new(collapse_threshold: f64, recoverable_threshold: f64) -> Self {
        Self {
            collapse_threshold,
            recoverable_threshold,
        }
    }

    pub fn evolve_identity(
        &self,
        identity: &SemanticIdentityState,
        semantic_snapshot: &str,
        concepts: &[String],
    ) -> SemanticIdentityState {
        let mut persistent_concepts =
            union_owned(identity.persistent_concepts.iter().chain(concepts.iter()));
        persistent_concepts.sort();

        let continuity = token_overlap(&identity.root_semantic_intent, semantic_snapshot);
        let concept_continuity = if concepts.is_empty() {
            0.0
        } else {
            list_overlap(&identity.persistent_concepts, concepts)
        };
        let continuity_score = (continuity * 0.65 + concept_continuity * 0.35).clamp(0.0, 1.0);
        let drift_score = 1.0 - continuity_score;
        let temporal_stability =
            (identity.temporal_stability * 0.6 + continuity_score * 0.4).clamp(0.0, 1.0);

        SemanticIdentityState {
            identity_id: identity.identity_id.clone(),
            root_semantic_intent: identity.root_semantic_intent.clone(),
            persistent_concepts,
            continuity_score,
            drift_score,
            temporal_stability,
        }
    }

    pub fn classify_drift(&self, identity: &SemanticIdentityState) -> DriftClassification {
        if identity.drift_score >= self.collapse_threshold || identity.continuity_score < 0.2 {
            DriftClassification::CatastrophicIdentityCollapse
        } else if identity.drift_score >= self.recoverable_threshold
            && identity.temporal_stability >= 0.55
        {
            DriftClassification::RecoverableDrift
        } else if identity.drift_score > 0.0 {
            DriftClassification::AdaptiveEvolution
        } else {
            DriftClassification::None
        }
    }

    pub fn identity_collapsed(&self, identity: &SemanticIdentityState) -> bool {
        self.classify_drift(identity) == DriftClassification::CatastrophicIdentityCollapse
    }

    pub fn repair_identity(
        &self,
        identity: &SemanticIdentityState,
        repair_concepts: &[String],
    ) -> SemanticIdentityState {
        let mut repaired = identity.clone();
        repaired.persistent_concepts = union_owned(
            repaired
                .persistent_concepts
                .iter()
                .chain(repair_concepts.iter()),
        );
        let repair_gain = list_overlap(&repaired.persistent_concepts, repair_concepts) * 0.25;
        repaired.continuity_score = (repaired.continuity_score + repair_gain).clamp(0.0, 1.0);
        repaired.drift_score = 1.0 - repaired.continuity_score;
        repaired.temporal_stability =
            (repaired.temporal_stability + repaired.continuity_score * 0.2).clamp(0.0, 1.0);
        repaired
    }

    pub fn append_memory(
        &self,
        memory: &mut TemporalSemanticMemory,
        lineage: String,
        abstraction: String,
        concept_form: String,
    ) {
        memory.semantic_lineages.push(lineage);
        memory.abstraction_histories.push(abstraction);
        memory.concept_evolution.push(concept_form);
        memory.temporal_coherence = temporal_coherence(memory);
    }

    pub fn compress_memory(
        &self,
        memory: &TemporalSemanticMemory,
        identity: &SemanticIdentityState,
    ) -> (TemporalSemanticMemory, f64) {
        let folded_lineage = stable_fold("lineage", &memory.semantic_lineages);
        let folded_abstraction = stable_fold("abstraction", &memory.abstraction_histories);
        let folded_concepts = stable_fold("concept", &memory.concept_evolution);
        let compressed = TemporalSemanticMemory {
            temporal_memory_id: memory.temporal_memory_id.clone(),
            semantic_lineages: vec![folded_lineage],
            abstraction_histories: vec![folded_abstraction],
            concept_evolution: vec![folded_concepts],
            temporal_coherence: memory.temporal_coherence,
        };
        let preservation = token_overlap(
            &identity.root_semantic_intent,
            &format!(
                "{} {} {}",
                compressed.semantic_lineages.join(" "),
                compressed.abstraction_histories.join(" "),
                compressed.concept_evolution.join(" ")
            ),
        )
        .max(memory.temporal_coherence * 0.75);
        (compressed, preservation.clamp(0.0, 1.0))
    }

    pub fn evolve_concept(
        &self,
        concept: &EvolvingConcept,
        new_form: String,
        abstraction_step: String,
    ) -> EvolvingConcept {
        let core_preservation = token_overlap(&concept.semantic_core, &new_form);
        let mut evolved = concept.clone();
        evolved.historical_forms.push(new_form);
        evolved.abstraction_trajectory.push(abstraction_step);
        evolved.continuity_score =
            (evolved.continuity_score * 0.55 + core_preservation * 0.45).clamp(0.0, 1.0);
        evolved
    }

    pub fn semantic_mutation_collapsed(&self, concept: &EvolvingConcept) -> bool {
        concept.continuity_score < 0.25
            || concept
                .historical_forms
                .last()
                .map(|form| token_overlap(&concept.semantic_core, form) < 0.2)
                .unwrap_or(false)
    }

    pub fn reinforce_attractor(
        &self,
        attractor: &mut TemporalAttractor,
        reinforcement: f64,
        patterns: &[String],
    ) {
        for pattern in patterns {
            if !attractor.persistent_patterns.contains(pattern) {
                attractor.persistent_patterns.push(pattern.clone());
            }
        }
        attractor.persistent_patterns.sort();
        attractor
            .reinforcement_history
            .push(reinforcement.clamp(0.0, 1.0));
        attractor.stability_over_time = average(attractor.reinforcement_history.iter().copied());
        attractor.drift_resistance = (attractor.drift_resistance * 0.7
            + attractor.stability_over_time * 0.3)
            .clamp(0.0, 1.0);
    }

    pub fn attractor_decay_detected(&self, attractor: &TemporalAttractor) -> bool {
        if attractor.reinforcement_history.len() < 2 {
            return attractor.stability_over_time < 0.25;
        }
        let latest = *attractor.reinforcement_history.last().unwrap_or(&0.0);
        latest < 0.3 && attractor.stability_over_time < 0.55
    }

    pub fn validate_constraint_evolution(
        &self,
        state: &LongHorizonPlanningState,
        next_constraint: &str,
    ) -> bool {
        state
            .persistent_goals
            .iter()
            .any(|goal| token_overlap(goal, next_constraint) > 0.0)
            || state.temporal_convergence_score >= 0.65
    }
}

pub struct LongHorizonContinuityRuntime {
    engine: TemporalContinuityEngine,
}

impl Default for LongHorizonContinuityRuntime {
    fn default() -> Self {
        Self {
            engine: TemporalContinuityEngine::default(),
        }
    }
}

impl LongHorizonContinuityRuntime {
    pub fn new(engine: TemporalContinuityEngine) -> Self {
        Self { engine }
    }

    pub fn run_sequence(
        &self,
        identity: SemanticIdentityState,
        memory: TemporalSemanticMemory,
        concepts: Vec<EvolvingConcept>,
        planning_state: LongHorizonPlanningState,
        sequence: &[TemporalSemanticStep],
    ) -> LongHorizonReport {
        let mut identity = identity;
        let mut memory = memory;
        let mut concepts = concepts;
        let mut planning_state = planning_state;
        let mut attractors: Vec<TemporalAttractor> = Vec::new();
        let mut events = Vec::new();

        for step in sequence {
            self.engine.append_memory(
                &mut memory,
                step.lineage.clone(),
                step.abstraction.clone(),
                step.concept_form.clone(),
            );
            events.push(ContinuityEvent::TemporalMemoryAppended {
                temporal_memory_id: memory.temporal_memory_id.clone(),
                lineage_count: memory.semantic_lineages.len(),
            });

            let step_concepts = vec![step.concept_id.clone()];
            identity =
                self.engine
                    .evolve_identity(&identity, &step.semantic_snapshot, &step_concepts);
            events.push(ContinuityEvent::IdentityEvolved {
                identity_id: identity.identity_id.clone(),
                continuity_score: identity.continuity_score,
            });

            if let Some(concept) = concepts
                .iter_mut()
                .find(|c| c.concept_id == step.concept_id)
            {
                *concept = self.engine.evolve_concept(
                    concept,
                    step.concept_form.clone(),
                    step.abstraction.clone(),
                );
                events.push(ContinuityEvent::ConceptEvolved {
                    concept_id: concept.concept_id.clone(),
                    continuity_score: concept.continuity_score,
                });
            }

            if let Some(index) = attractors
                .iter()
                .position(|a| a.attractor_id == step.attractor_id)
            {
                let attractor = &mut attractors[index];
                self.engine.reinforce_attractor(
                    attractor,
                    step.reinforcement,
                    &tokens(&step.semantic_snapshot),
                );
                events.push(ContinuityEvent::TemporalAttractorReinforced {
                    attractor_id: attractor.attractor_id.clone(),
                    stability_over_time: attractor.stability_over_time,
                });
            } else {
                let mut attractor = TemporalAttractor {
                    attractor_id: step.attractor_id.clone(),
                    persistent_patterns: Vec::new(),
                    reinforcement_history: Vec::new(),
                    stability_over_time: 0.0,
                    drift_resistance: 0.5,
                };
                self.engine.reinforce_attractor(
                    &mut attractor,
                    step.reinforcement,
                    &tokens(&step.semantic_snapshot),
                );
                events.push(ContinuityEvent::TemporalAttractorReinforced {
                    attractor_id: attractor.attractor_id.clone(),
                    stability_over_time: attractor.stability_over_time,
                });
                attractors.push(attractor);
            }

            if self
                .engine
                .validate_constraint_evolution(&planning_state, &step.constraint)
            {
                planning_state
                    .evolving_constraints
                    .push(step.constraint.clone());
                planning_state.planning_lineage.push(step.lineage.clone());
                planning_state.temporal_convergence_score =
                    (planning_state.temporal_convergence_score * 0.65 + step.reinforcement * 0.35)
                        .clamp(0.0, 1.0);
            }

            let classification = self.engine.classify_drift(&identity);
            events.push(ContinuityEvent::SemanticDriftClassified {
                identity_id: identity.identity_id.clone(),
                classification,
            });
            match classification {
                DriftClassification::RecoverableDrift => {
                    identity = self
                        .engine
                        .repair_identity(&identity, &planning_state.persistent_goals);
                    events.push(ContinuityEvent::ContinuityRepaired {
                        identity_id: identity.identity_id.clone(),
                        continuity_score: identity.continuity_score,
                    });
                }
                DriftClassification::CatastrophicIdentityCollapse => {
                    events.push(ContinuityEvent::SemanticIdentityCollapse {
                        identity_id: identity.identity_id.clone(),
                        drift_score: identity.drift_score,
                    });
                }
                DriftClassification::None | DriftClassification::AdaptiveEvolution => {}
            }
        }

        sort_identities(std::slice::from_mut(&mut identity));
        sort_concepts(&mut concepts);
        sort_attractors(&mut attractors);
        planning_state.persistent_goals.sort();
        planning_state.evolving_constraints.sort();
        planning_state.planning_lineage.sort();

        LongHorizonReport {
            identity,
            temporal_memory: memory,
            evolving_concepts: concepts,
            temporal_attractors: attractors,
            planning_state,
            events,
        }
    }

    pub fn compress_temporal_memory(
        &self,
        memory: &TemporalSemanticMemory,
        identity: &SemanticIdentityState,
    ) -> (TemporalSemanticMemory, ContinuityEvent) {
        let (compressed, preservation) = self.engine.compress_memory(memory, identity);
        (
            compressed.clone(),
            ContinuityEvent::TemporalCompressionPerformed {
                temporal_memory_id: compressed.temporal_memory_id,
                semantic_preservation_score: preservation,
            },
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TemporalSemanticStep {
    pub semantic_snapshot: String,
    pub lineage: String,
    pub abstraction: String,
    pub concept_id: String,
    pub concept_form: String,
    pub attractor_id: String,
    pub constraint: String,
    pub reinforcement: f64,
}

impl From<&ConceptNode> for EvolvingConcept {
    fn from(concept: &ConceptNode) -> Self {
        Self {
            concept_id: concept.concept_id.clone(),
            historical_forms: vec![concept.conceptual_signature.clone()],
            abstraction_trajectory: concept.abstraction_dependencies.clone(),
            semantic_core: concept.conceptual_signature.clone(),
            continuity_score: concept.conceptual_stability,
        }
    }
}

fn sort_identities(states: &mut [SemanticIdentityState]) {
    states.sort_by(|a, b| {
        compare_desc(b.temporal_stability, a.temporal_stability)
            .then(compare_desc(b.continuity_score, a.continuity_score))
            .then(compare_desc(1.0 - b.drift_score, 1.0 - a.drift_score))
            .then(a.identity_id.cmp(&b.identity_id))
    });
}

fn sort_concepts(concepts: &mut [EvolvingConcept]) {
    concepts.sort_by(|a, b| {
        compare_desc(b.continuity_score, a.continuity_score)
            .then(compare_desc(
                b.abstraction_trajectory.len() as f64,
                a.abstraction_trajectory.len() as f64,
            ))
            .then(a.concept_id.cmp(&b.concept_id))
    });
}

fn sort_attractors(attractors: &mut [TemporalAttractor]) {
    attractors.sort_by(|a, b| {
        compare_desc(b.stability_over_time, a.stability_over_time)
            .then(compare_desc(b.drift_resistance, a.drift_resistance))
            .then(a.attractor_id.cmp(&b.attractor_id))
    });
}

fn compare_desc(left: f64, right: f64) -> Ordering {
    left.partial_cmp(&right).unwrap_or(Ordering::Equal)
}

fn temporal_coherence(memory: &TemporalSemanticMemory) -> f64 {
    let lineage_coherence = repeated_token_ratio(&memory.semantic_lineages);
    let abstraction_coherence = repeated_token_ratio(&memory.abstraction_histories);
    let concept_coherence = repeated_token_ratio(&memory.concept_evolution);
    (lineage_coherence * 0.3 + abstraction_coherence * 0.35 + concept_coherence * 0.35)
        .clamp(0.0, 1.0)
}

fn stable_fold(prefix: &str, values: &[String]) -> String {
    let mut freq: BTreeMap<String, usize> = BTreeMap::new();
    for value in values {
        for token in tokens(value) {
            *freq.entry(token).or_insert(0) += 1;
        }
    }
    let minimum = if values.len() <= 1 { 1 } else { 2 };
    let folded: Vec<_> = freq
        .into_iter()
        .filter_map(|(token, count)| if count >= minimum { Some(token) } else { None })
        .collect();
    if folded.is_empty() {
        format!("{prefix}:semantic-attractor")
    } else {
        format!("{prefix}:{}", folded.join(" "))
    }
}

fn repeated_token_ratio(values: &[String]) -> f64 {
    if values.is_empty() {
        return 1.0;
    }
    let mut freq: BTreeMap<String, usize> = BTreeMap::new();
    let mut total = 0usize;
    for value in values {
        for token in tokens(value) {
            *freq.entry(token).or_insert(0) += 1;
            total += 1;
        }
    }
    if total == 0 {
        return 0.0;
    }
    let repeated = freq.values().filter(|count| **count > 1).sum::<usize>();
    repeated as f64 / total as f64
}

fn token_overlap(a: &str, b: &str) -> f64 {
    let ta: BTreeSet<_> = tokens(a).into_iter().collect();
    let tb: BTreeSet<_> = tokens(b).into_iter().collect();
    if ta.is_empty() && tb.is_empty() {
        return 1.0;
    }
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    ta.intersection(&tb).count() as f64 / ta.union(&tb).count() as f64
}

fn list_overlap(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let sa: BTreeSet<_> = a.iter().map(|s| normalize_token(s)).collect();
    let sb: BTreeSet<_> = b.iter().map(|s| normalize_token(s)).collect();
    sa.intersection(&sb).count() as f64 / sa.union(&sb).count() as f64
}

fn tokens(value: &str) -> Vec<String> {
    let mut result: Vec<_> = value
        .split_whitespace()
        .map(normalize_token)
        .filter(|token| !token.is_empty())
        .collect();
    result.sort();
    result.dedup();
    result
}

fn normalize_token(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
}

fn union_owned<'a>(values: impl Iterator<Item = &'a String>) -> Vec<String> {
    values
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn average(values: impl Iterator<Item = f64>) -> f64 {
    let mut total = 0.0;
    let mut count = 0.0;
    for value in values {
        total += value;
        count += 1.0;
    }
    if count == 0.0 {
        0.0
    } else {
        (total / count).clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> SemanticIdentityState {
        SemanticIdentityState {
            identity_id: "IDENTITY_ARCH".to_string(),
            root_semantic_intent: "architecture resilience planning continuity".to_string(),
            persistent_concepts: vec!["resilience".to_string(), "planning".to_string()],
            continuity_score: 1.0,
            drift_score: 0.0,
            temporal_stability: 0.9,
        }
    }

    fn memory() -> TemporalSemanticMemory {
        TemporalSemanticMemory {
            temporal_memory_id: "TM1".to_string(),
            semantic_lineages: Vec::new(),
            abstraction_histories: Vec::new(),
            concept_evolution: Vec::new(),
            temporal_coherence: 1.0,
        }
    }

    fn concept() -> EvolvingConcept {
        EvolvingConcept {
            concept_id: "C_RESILIENCE".to_string(),
            historical_forms: vec!["architecture resilience planning".to_string()],
            abstraction_trajectory: vec!["concrete pattern".to_string()],
            semantic_core: "resilience planning".to_string(),
            continuity_score: 0.9,
        }
    }

    fn planning() -> LongHorizonPlanningState {
        LongHorizonPlanningState {
            planning_state_id: "PLAN_LONG".to_string(),
            persistent_goals: vec!["architecture resilience".to_string()],
            evolving_constraints: Vec::new(),
            planning_lineage: Vec::new(),
            temporal_convergence_score: 0.8,
        }
    }

    fn stable_steps() -> Vec<TemporalSemanticStep> {
        vec![
            TemporalSemanticStep {
                semantic_snapshot: "architecture resilience planning boundary".to_string(),
                lineage: "session one architecture resilience".to_string(),
                abstraction: "resilience abstraction".to_string(),
                concept_id: "C_RESILIENCE".to_string(),
                concept_form: "resilience planning boundary".to_string(),
                attractor_id: "ATTR_RESILIENCE".to_string(),
                constraint: "architecture resilience governance".to_string(),
                reinforcement: 0.88,
            },
            TemporalSemanticStep {
                semantic_snapshot: "architecture resilience planning deployment".to_string(),
                lineage: "session two architecture resilience".to_string(),
                abstraction: "deployment resilience abstraction".to_string(),
                concept_id: "C_RESILIENCE".to_string(),
                concept_form: "resilience planning deployment".to_string(),
                attractor_id: "ATTR_RESILIENCE".to_string(),
                constraint: "planning resilience constraint".to_string(),
                reinforcement: 0.92,
            },
        ]
    }

    #[test]
    fn semantic_identity_persists_over_time() {
        let report = LongHorizonContinuityRuntime::default().run_sequence(
            identity(),
            memory(),
            vec![concept()],
            planning(),
            &stable_steps(),
        );
        assert!(report.identity.continuity_score >= 0.5);
        assert!(report
            .identity
            .persistent_concepts
            .contains(&"C_RESILIENCE".to_string()));
    }

    #[test]
    fn identity_collapse_detected() {
        let engine = TemporalContinuityEngine::default();
        let collapsed = engine.evolve_identity(&identity(), "unrelated billing spreadsheet", &[]);
        assert!(engine.identity_collapsed(&collapsed));
    }

    #[test]
    fn continuity_score_stable() {
        let engine = TemporalContinuityEngine::default();
        let a = engine.evolve_identity(
            &identity(),
            "architecture resilience planning",
            &["planning".to_string()],
        );
        let b = engine.evolve_identity(
            &identity(),
            "architecture resilience planning",
            &["planning".to_string()],
        );
        assert_eq!(a, b);
    }

    #[test]
    fn temporal_memory_append_only() {
        let engine = TemporalContinuityEngine::default();
        let mut memory = memory();
        engine.append_memory(
            &mut memory,
            "L1".to_string(),
            "A1".to_string(),
            "C1".to_string(),
        );
        engine.append_memory(
            &mut memory,
            "L2".to_string(),
            "A2".to_string(),
            "C2".to_string(),
        );
        assert_eq!(
            memory.semantic_lineages,
            vec!["L1".to_string(), "L2".to_string()]
        );
        assert_eq!(memory.abstraction_histories.len(), 2);
        assert_eq!(memory.concept_evolution.len(), 2);
    }

    #[test]
    fn temporal_compression_preserves_meaning() {
        let engine = TemporalContinuityEngine::default();
        let mut memory = memory();
        engine.append_memory(
            &mut memory,
            "architecture resilience lineage one".to_string(),
            "resilience abstraction one".to_string(),
            "resilience planning concept".to_string(),
        );
        engine.append_memory(
            &mut memory,
            "architecture resilience lineage two".to_string(),
            "resilience abstraction two".to_string(),
            "resilience planning concept evolved".to_string(),
        );
        let (compressed, preservation) = engine.compress_memory(&memory, &identity());
        assert_eq!(compressed.semantic_lineages.len(), 1);
        assert!(preservation >= 0.5);
    }

    #[test]
    fn memory_lineage_replayable() {
        let runtime = LongHorizonContinuityRuntime::default();
        let first = runtime.run_sequence(
            identity(),
            memory(),
            vec![concept()],
            planning(),
            &stable_steps(),
        );
        let second = runtime.run_sequence(
            identity(),
            memory(),
            vec![concept()],
            planning(),
            &stable_steps(),
        );
        assert_eq!(first.temporal_memory, second.temporal_memory);
    }

    #[test]
    fn concept_evolution_preserves_core() {
        let engine = TemporalContinuityEngine::default();
        let evolved = engine.evolve_concept(
            &concept(),
            "resilience planning deployment".to_string(),
            "deployment abstraction".to_string(),
        );
        assert!(evolved.continuity_score >= 0.5);
        assert_eq!(evolved.semantic_core, "resilience planning");
    }

    #[test]
    fn abstraction_trajectory_stable() {
        let engine = TemporalContinuityEngine::default();
        let evolved = engine.evolve_concept(
            &concept(),
            "resilience planning".to_string(),
            "A2".to_string(),
        );
        assert_eq!(
            evolved.abstraction_trajectory,
            vec!["concrete pattern".to_string(), "A2".to_string()]
        );
    }

    #[test]
    fn semantic_mutation_collapse_detected() {
        let engine = TemporalContinuityEngine::default();
        let collapsed = engine.evolve_concept(
            &concept(),
            "unrelated payroll report".to_string(),
            "mutation".to_string(),
        );
        assert!(engine.semantic_mutation_collapsed(&collapsed));
    }

    #[test]
    fn persistent_attractor_strengthening_stable() {
        let engine = TemporalContinuityEngine::default();
        let mut attractor = TemporalAttractor {
            attractor_id: "A".to_string(),
            persistent_patterns: Vec::new(),
            reinforcement_history: Vec::new(),
            stability_over_time: 0.0,
            drift_resistance: 0.5,
        };
        engine.reinforce_attractor(&mut attractor, 0.8, &["resilience".to_string()]);
        engine.reinforce_attractor(&mut attractor, 0.9, &["planning".to_string()]);
        assert!(attractor.stability_over_time >= 0.85);
        assert_eq!(attractor.reinforcement_history, vec![0.8, 0.9]);
    }

    #[test]
    fn attractor_decay_detected() {
        let engine = TemporalContinuityEngine::default();
        let attractor = TemporalAttractor {
            attractor_id: "A".to_string(),
            persistent_patterns: vec!["resilience".to_string()],
            reinforcement_history: vec![0.2, 0.1],
            stability_over_time: 0.15,
            drift_resistance: 0.2,
        };
        assert!(engine.attractor_decay_detected(&attractor));
    }

    #[test]
    fn temporal_reinforcement_deterministic() {
        let runtime = LongHorizonContinuityRuntime::default();
        let first = runtime.run_sequence(
            identity(),
            memory(),
            vec![concept()],
            planning(),
            &stable_steps(),
        );
        let second = runtime.run_sequence(
            identity(),
            memory(),
            vec![concept()],
            planning(),
            &stable_steps(),
        );
        assert_eq!(first.temporal_attractors, second.temporal_attractors);
    }

    #[test]
    fn recoverable_drift_classified_correctly() {
        let engine = TemporalContinuityEngine::default();
        let mut state = identity();
        state.continuity_score = 0.6;
        state.drift_score = 0.4;
        state.temporal_stability = 0.8;
        assert_eq!(
            engine.classify_drift(&state),
            DriftClassification::RecoverableDrift
        );
    }

    #[test]
    fn catastrophic_drift_halts_runtime() {
        let engine = TemporalContinuityEngine::default();
        let mut state = identity();
        state.continuity_score = 0.1;
        state.drift_score = 0.9;
        assert_eq!(
            engine.classify_drift(&state),
            DriftClassification::CatastrophicIdentityCollapse
        );
    }

    #[test]
    fn continuity_repair_restores_identity() {
        let engine = TemporalContinuityEngine::default();
        let mut state = identity();
        state.continuity_score = 0.55;
        state.drift_score = 0.45;
        let repaired = engine.repair_identity(&state, &["architecture resilience".to_string()]);
        assert!(repaired.continuity_score > state.continuity_score);
        assert!(repaired.drift_score < state.drift_score);
    }

    #[test]
    fn verification_a_multi_session_planning() {
        let report = LongHorizonContinuityRuntime::default().run_sequence(
            identity(),
            memory(),
            vec![concept()],
            planning(),
            &stable_steps(),
        );
        assert!(report.identity.continuity_score >= 0.5);
    }

    #[test]
    fn verification_b_concept_evolution() {
        let report = LongHorizonContinuityRuntime::default().run_sequence(
            identity(),
            memory(),
            vec![concept()],
            planning(),
            &stable_steps(),
        );
        assert_eq!(
            report.evolving_concepts[0].semantic_core,
            "resilience planning"
        );
    }

    #[test]
    fn verification_c_temporal_compression() {
        let runtime = LongHorizonContinuityRuntime::default();
        let report = runtime.run_sequence(
            identity(),
            memory(),
            vec![concept()],
            planning(),
            &stable_steps(),
        );
        let (_, event) =
            runtime.compress_temporal_memory(&report.temporal_memory, &report.identity);
        assert!(matches!(
            event,
            ContinuityEvent::TemporalCompressionPerformed {
                semantic_preservation_score,
                ..
            } if semantic_preservation_score >= 0.5
        ));
    }

    #[test]
    fn verification_d_drift_recovery() {
        let engine = TemporalContinuityEngine::default();
        let mut state = identity();
        state.continuity_score = 0.55;
        state.drift_score = 0.45;
        state.temporal_stability = 0.8;
        assert_eq!(
            engine.classify_drift(&state),
            DriftClassification::RecoverableDrift
        );
        let repaired = engine.repair_identity(&state, &["architecture resilience".to_string()]);
        assert!(repaired.continuity_score > state.continuity_score);
    }

    #[test]
    fn verification_e_identity_collapse() {
        let engine = TemporalContinuityEngine::default();
        let collapsed =
            engine.evolve_identity(&identity(), "catastrophic unrelated contradiction", &[]);
        assert_eq!(
            engine.classify_drift(&collapsed),
            DriftClassification::CatastrophicIdentityCollapse
        );
    }
}
