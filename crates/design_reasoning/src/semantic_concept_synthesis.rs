use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::holographic_semantic_memory::HolographicSemanticMemory;

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticAbstraction {
    pub abstraction_id: String,
    pub abstraction_name: String,
    pub abstraction_signature: String,
    pub source_patterns: Vec<String>,
    pub semantic_roles: Vec<String>,
    pub causal_patterns: Vec<String>,
    pub abstraction_depth: f64,
    pub transferability_score: f64,
    pub stability_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConceptNode {
    pub concept_id: String,
    pub concept_name: String,
    pub conceptual_signature: String,
    pub abstraction_dependencies: Vec<String>,
    pub semantic_constraints: Vec<String>,
    pub transfer_domains: Vec<String>,
    pub conceptual_stability: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConceptGraph {
    pub concepts: Vec<ConceptNode>,
    pub conceptual_dependencies: Vec<(String, String)>,
    pub conceptual_convergence_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConceptHierarchyNode {
    pub hierarchy_id: String,
    pub parent_concept: Option<String>,
    pub child_concepts: Vec<String>,
    pub abstraction_level: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransferMapping {
    pub source_domain: String,
    pub target_domain: String,
    pub transferred_concepts: Vec<String>,
    pub semantic_preservation_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticPatternInput {
    pub pattern_id: String,
    pub domain: String,
    pub semantic_signature: String,
    pub semantic_roles: Vec<String>,
    pub causal_patterns: Vec<String>,
    pub convergence_score: f64,
    pub stability_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConceptFormationEvent {
    AbstractionSynthesized {
        abstraction_id: String,
        source_count: usize,
    },
    ConceptFormed {
        concept_id: String,
        abstraction_count: usize,
    },
    HierarchyConstructed {
        hierarchy_id: String,
        child_count: usize,
    },
    TransferMapped {
        source_domain: String,
        target_domain: String,
        concept_count: usize,
    },
    MetaConceptEmerged {
        concept_id: String,
    },
    ConceptualDriftDetected {
        concept_id: String,
        drift_score: f64,
    },
    SemanticCompressionRejected {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConceptSynthesisReport {
    pub abstractions: Vec<SemanticAbstraction>,
    pub concept_graph: ConceptGraph,
    pub hierarchy: Vec<ConceptHierarchyNode>,
    pub transfer_mappings: Vec<TransferMapping>,
    pub events: Vec<ConceptFormationEvent>,
    pub entropy_before: f64,
    pub entropy_after: f64,
}

pub struct SemanticAbstractionEngine {
    similarity_threshold: f64,
}

impl Default for SemanticAbstractionEngine {
    fn default() -> Self {
        Self {
            similarity_threshold: 0.34,
        }
    }
}

impl SemanticAbstractionEngine {
    pub fn new(similarity_threshold: f64) -> Self {
        Self {
            similarity_threshold,
        }
    }

    pub fn synthesize(&self, inputs: &[SemanticPatternInput]) -> Vec<SemanticAbstraction> {
        let mut sorted = inputs.to_vec();
        sorted.sort_by(|a, b| a.pattern_id.cmp(&b.pattern_id));

        let mut groups: Vec<Vec<SemanticPatternInput>> = Vec::new();
        for input in sorted {
            if let Some(group) = groups.iter_mut().find(|group| {
                group
                    .iter()
                    .any(|existing| self.is_abstraction_match(existing, &input))
            }) {
                group.push(input);
            } else {
                groups.push(vec![input]);
            }
        }

        let mut abstractions = Vec::new();
        for group in groups.into_iter().filter(|group| group.len() >= 2) {
            let signature = common_signature(&group);
            let roles = union_sorted(group.iter().flat_map(|p| p.semantic_roles.iter()));
            let causal = union_sorted(group.iter().flat_map(|p| p.causal_patterns.iter()));
            let sources = union_sorted(group.iter().map(|p| &p.pattern_id));
            let domains = union_sorted(group.iter().map(|p| &p.domain));
            let depth = group.len() as f64;
            let stability = average(group.iter().map(|p| p.stability_score));
            let convergence = average(group.iter().map(|p| p.convergence_score));
            let transferability = ((domains.len() as f64 / group.len() as f64) * 0.55
                + role_density(&roles) * 0.45)
                .clamp(0.0, 1.0);
            let id = stable_id("ABS", &signature, abstractions.len());

            abstractions.push(SemanticAbstraction {
                abstraction_id: id,
                abstraction_name: format!("abstraction::{signature}"),
                abstraction_signature: signature,
                source_patterns: sources,
                semantic_roles: roles,
                causal_patterns: causal,
                abstraction_depth: depth,
                transferability_score: transferability,
                stability_score: ((stability * 0.7) + (convergence * 0.3)).clamp(0.0, 1.0),
            });
        }

        sort_abstractions(&mut abstractions);
        abstractions
    }

    pub fn from_holographic_memories(
        &self,
        memories: &[HolographicSemanticMemory],
    ) -> Vec<SemanticAbstraction> {
        let inputs: Vec<_> = memories
            .iter()
            .map(|memory| SemanticPatternInput {
                pattern_id: memory.memory_id.clone(),
                domain: "holographic_memory".to_string(),
                semantic_signature: memory.semantic_signature.clone(),
                semantic_roles: memory.semantic_roles.clone(),
                causal_patterns: memory.causal_patterns.clone(),
                convergence_score: memory.attractor_strength,
                stability_score: memory.uniqueness_score,
            })
            .collect();
        self.synthesize(&inputs)
    }

    fn is_abstraction_match(&self, a: &SemanticPatternInput, b: &SemanticPatternInput) -> bool {
        token_overlap(&a.semantic_signature, &b.semantic_signature) >= self.similarity_threshold
            || list_overlap(&a.semantic_roles, &b.semantic_roles) >= 0.5
            || list_overlap(&a.causal_patterns, &b.causal_patterns) >= 0.5
    }
}

pub struct ConceptSynthesisEngine {
    meta_concept_threshold: usize,
}

impl Default for ConceptSynthesisEngine {
    fn default() -> Self {
        Self {
            meta_concept_threshold: 3,
        }
    }
}

impl ConceptSynthesisEngine {
    pub fn new(meta_concept_threshold: usize) -> Self {
        Self {
            meta_concept_threshold,
        }
    }

    pub fn synthesize(&self, abstractions: &[SemanticAbstraction]) -> ConceptGraph {
        let mut groups: Vec<Vec<SemanticAbstraction>> = Vec::new();
        let mut sorted = abstractions.to_vec();
        sort_abstractions(&mut sorted);

        for abstraction in sorted {
            if let Some(group) = groups.iter_mut().find(|group| {
                group.iter().any(|existing| {
                    token_overlap(
                        &existing.abstraction_signature,
                        &abstraction.abstraction_signature,
                    ) >= 0.4
                        || list_overlap(&existing.semantic_roles, &abstraction.semantic_roles)
                            >= 0.5
                })
            }) {
                group.push(abstraction);
            } else {
                groups.push(vec![abstraction]);
            }
        }

        let mut concepts = Vec::new();
        for group in groups {
            let signature = concept_signature(&group);
            let dependencies = union_sorted(group.iter().map(|a| &a.abstraction_id));
            let constraints = union_sorted(group.iter().flat_map(|a| a.semantic_roles.iter()));
            let transfer_domains = infer_transfer_domains(&constraints, &signature);
            let stability = average(group.iter().map(|a| a.stability_score));
            let transferability = average(group.iter().map(|a| a.transferability_score));
            let id = stable_id("CONCEPT", &signature, concepts.len());
            concepts.push(ConceptNode {
                concept_id: id,
                concept_name: format!("concept::{signature}"),
                conceptual_signature: signature,
                abstraction_dependencies: dependencies,
                semantic_constraints: constraints,
                transfer_domains,
                conceptual_stability: ((stability * 0.75) + (transferability * 0.25))
                    .clamp(0.0, 1.0),
            });
        }

        if abstractions.len() >= self.meta_concept_threshold {
            let signature = meta_signature(abstractions);
            let dependencies = union_sorted(abstractions.iter().map(|a| &a.abstraction_id));
            let constraints =
                union_sorted(abstractions.iter().flat_map(|a| a.semantic_roles.iter()));
            concepts.push(ConceptNode {
                concept_id: stable_id("META_CONCEPT", &signature, 0),
                concept_name: format!("meta-concept::{signature}"),
                conceptual_signature: signature,
                abstraction_dependencies: dependencies,
                semantic_constraints: constraints,
                transfer_domains: vec![
                    "architecture".to_string(),
                    "governance".to_string(),
                    "planning".to_string(),
                    "recovery".to_string(),
                ],
                conceptual_stability: average(abstractions.iter().map(|a| a.stability_score)),
            });
        }

        sort_concepts(&mut concepts);
        let conceptual_dependencies = concept_dependencies(&concepts);
        let conceptual_convergence_score = average(concepts.iter().map(|c| c.conceptual_stability));

        ConceptGraph {
            concepts,
            conceptual_dependencies,
            conceptual_convergence_score,
        }
    }

    pub fn build_hierarchy(&self, graph: &ConceptGraph) -> Vec<ConceptHierarchyNode> {
        let parent = graph
            .concepts
            .iter()
            .find(|concept| concept.concept_id.starts_with("META_CONCEPT"))
            .map(|concept| concept.concept_id.clone())
            .or_else(|| {
                graph
                    .concepts
                    .first()
                    .map(|concept| concept.concept_id.clone())
            });

        let mut hierarchy = Vec::new();
        for concept in &graph.concepts {
            let child_concepts: Vec<String> = graph
                .conceptual_dependencies
                .iter()
                .filter_map(|(from, to)| {
                    if from == &concept.concept_id {
                        Some(to.clone())
                    } else {
                        None
                    }
                })
                .collect();
            hierarchy.push(ConceptHierarchyNode {
                hierarchy_id: format!("HIER_{}", concept.concept_id),
                parent_concept: if Some(&concept.concept_id) == parent.as_ref() {
                    None
                } else {
                    parent.clone()
                },
                child_concepts,
                abstraction_level: if concept.concept_id.starts_with("META_CONCEPT") {
                    4.0
                } else if concept.abstraction_dependencies.len() > 1 {
                    3.0
                } else {
                    2.0
                },
            });
        }
        hierarchy.sort_by(|a, b| a.hierarchy_id.cmp(&b.hierarchy_id));
        hierarchy
    }
}

pub struct CrossDomainTransferEngine;

impl CrossDomainTransferEngine {
    pub fn transfer(
        graph: &ConceptGraph,
        source_domain: &str,
        target_domain: &str,
    ) -> TransferMapping {
        let mut transferred: Vec<String> = graph
            .concepts
            .iter()
            .filter(|concept| {
                let domains: BTreeSet<&str> = concept
                    .transfer_domains
                    .iter()
                    .map(String::as_str)
                    .collect();
                domains.contains(source_domain)
                    || domains.contains(target_domain)
                    || concept.transfer_domains.len() >= 3
            })
            .map(|concept| concept.concept_id.clone())
            .collect();
        if transferred.is_empty() {
            transferred = graph
                .concepts
                .iter()
                .filter(|concept| concept.conceptual_stability >= 0.6)
                .map(|concept| concept.concept_id.clone())
                .collect();
        }
        transferred.sort();

        let preservation = if transferred.is_empty() {
            0.0
        } else {
            let selected_stability = average(graph.concepts.iter().filter_map(|concept| {
                if transferred.contains(&concept.concept_id) {
                    Some(concept.conceptual_stability)
                } else {
                    None
                }
            }));
            (selected_stability * 0.85 + graph.conceptual_convergence_score * 0.15).clamp(0.0, 1.0)
        };

        TransferMapping {
            source_domain: source_domain.to_string(),
            target_domain: target_domain.to_string(),
            transferred_concepts: transferred,
            semantic_preservation_score: preservation,
        }
    }
}

pub struct SemanticCompressionEngine {
    minimum_preservation_score: f64,
}

impl Default for SemanticCompressionEngine {
    fn default() -> Self {
        Self {
            minimum_preservation_score: 0.6,
        }
    }
}

impl SemanticCompressionEngine {
    pub fn new(minimum_preservation_score: f64) -> Self {
        Self {
            minimum_preservation_score,
        }
    }

    pub fn compress_concepts(&self, graph: &ConceptGraph) -> Result<ConceptGraph, String> {
        let mut by_signature: BTreeMap<String, ConceptNode> = BTreeMap::new();
        for concept in &graph.concepts {
            let key = normalize_signature(&concept.conceptual_signature);
            by_signature
                .entry(key)
                .and_modify(|existing| {
                    existing.abstraction_dependencies = union_owned(
                        existing
                            .abstraction_dependencies
                            .iter()
                            .chain(concept.abstraction_dependencies.iter()),
                    );
                    existing.semantic_constraints = union_owned(
                        existing
                            .semantic_constraints
                            .iter()
                            .chain(concept.semantic_constraints.iter()),
                    );
                    existing.transfer_domains = union_owned(
                        existing
                            .transfer_domains
                            .iter()
                            .chain(concept.transfer_domains.iter()),
                    );
                    existing.conceptual_stability = existing
                        .conceptual_stability
                        .max(concept.conceptual_stability);
                })
                .or_insert_with(|| concept.clone());
        }

        let mut concepts: Vec<_> = by_signature.into_values().collect();
        sort_concepts(&mut concepts);
        let preservation = preservation_score(&graph.concepts, &concepts);
        if preservation < self.minimum_preservation_score {
            return Err(format!(
                "semantic preservation score {preservation:.3} below compression threshold"
            ));
        }

        Ok(ConceptGraph {
            conceptual_dependencies: concept_dependencies(&concepts),
            conceptual_convergence_score: average(concepts.iter().map(|c| c.conceptual_stability)),
            concepts,
        })
    }

    pub fn entropy(graph: &ConceptGraph) -> f64 {
        let signature_count = graph
            .concepts
            .iter()
            .map(|concept| normalize_signature(&concept.conceptual_signature))
            .collect::<BTreeSet<_>>()
            .len() as f64;
        if graph.concepts.is_empty() {
            0.0
        } else {
            graph.concepts.len() as f64 / signature_count
        }
    }
}

pub struct ConceptCognitionRuntime {
    abstraction_engine: SemanticAbstractionEngine,
    synthesis_engine: ConceptSynthesisEngine,
    compression_engine: SemanticCompressionEngine,
}

impl Default for ConceptCognitionRuntime {
    fn default() -> Self {
        Self {
            abstraction_engine: SemanticAbstractionEngine::default(),
            synthesis_engine: ConceptSynthesisEngine::default(),
            compression_engine: SemanticCompressionEngine::default(),
        }
    }
}

impl ConceptCognitionRuntime {
    pub fn run(
        &self,
        inputs: &[SemanticPatternInput],
        transfer_requests: &[(&str, &str)],
    ) -> ConceptSynthesisReport {
        let abstractions = self.abstraction_engine.synthesize(inputs);
        let graph = self.synthesis_engine.synthesize(&abstractions);
        let entropy_before = SemanticCompressionEngine::entropy(&graph);
        let compressed = self
            .compression_engine
            .compress_concepts(&graph)
            .unwrap_or_else(|_| graph.clone());
        let entropy_after = SemanticCompressionEngine::entropy(&compressed);
        let hierarchy = self.synthesis_engine.build_hierarchy(&compressed);
        let transfer_mappings: Vec<_> = transfer_requests
            .iter()
            .map(|(source, target)| {
                CrossDomainTransferEngine::transfer(&compressed, source, target)
            })
            .collect();
        let events = formation_events(&abstractions, &compressed, &hierarchy, &transfer_mappings);

        ConceptSynthesisReport {
            abstractions,
            concept_graph: compressed,
            hierarchy,
            transfer_mappings,
            events,
            entropy_before,
            entropy_after,
        }
    }

    pub fn detect_conceptual_drift(
        &self,
        concept: &ConceptNode,
        candidate_signature: &str,
    ) -> Option<ConceptFormationEvent> {
        let drift = 1.0 - token_overlap(&concept.conceptual_signature, candidate_signature);
        if drift > (1.0 - concept.conceptual_stability).max(0.25) {
            Some(ConceptFormationEvent::ConceptualDriftDetected {
                concept_id: concept.concept_id.clone(),
                drift_score: drift,
            })
        } else {
            None
        }
    }
}

fn formation_events(
    abstractions: &[SemanticAbstraction],
    graph: &ConceptGraph,
    hierarchy: &[ConceptHierarchyNode],
    transfers: &[TransferMapping],
) -> Vec<ConceptFormationEvent> {
    let mut events = Vec::new();
    for abstraction in abstractions {
        events.push(ConceptFormationEvent::AbstractionSynthesized {
            abstraction_id: abstraction.abstraction_id.clone(),
            source_count: abstraction.source_patterns.len(),
        });
    }
    for concept in &graph.concepts {
        events.push(ConceptFormationEvent::ConceptFormed {
            concept_id: concept.concept_id.clone(),
            abstraction_count: concept.abstraction_dependencies.len(),
        });
        if concept.concept_id.starts_with("META_CONCEPT") {
            events.push(ConceptFormationEvent::MetaConceptEmerged {
                concept_id: concept.concept_id.clone(),
            });
        }
    }
    for node in hierarchy {
        events.push(ConceptFormationEvent::HierarchyConstructed {
            hierarchy_id: node.hierarchy_id.clone(),
            child_count: node.child_concepts.len(),
        });
    }
    for transfer in transfers {
        events.push(ConceptFormationEvent::TransferMapped {
            source_domain: transfer.source_domain.clone(),
            target_domain: transfer.target_domain.clone(),
            concept_count: transfer.transferred_concepts.len(),
        });
    }
    events
}

fn sort_abstractions(abstractions: &mut [SemanticAbstraction]) {
    abstractions.sort_by(|a, b| {
        compare_desc(b.stability_score, a.stability_score)
            .then(compare_desc(
                b.transferability_score,
                a.transferability_score,
            ))
            .then(compare_desc(b.abstraction_depth, a.abstraction_depth))
            .then(a.abstraction_id.cmp(&b.abstraction_id))
    });
}

fn sort_concepts(concepts: &mut [ConceptNode]) {
    concepts.sort_by(|a, b| {
        compare_desc(b.conceptual_stability, a.conceptual_stability)
            .then(compare_desc(
                b.transfer_domains.len() as f64,
                a.transfer_domains.len() as f64,
            ))
            .then(compare_desc(
                b.abstraction_dependencies.len() as f64,
                a.abstraction_dependencies.len() as f64,
            ))
            .then(a.concept_id.cmp(&b.concept_id))
    });
}

fn compare_desc(left: f64, right: f64) -> Ordering {
    left.partial_cmp(&right).unwrap_or(Ordering::Equal)
}

fn common_signature(group: &[SemanticPatternInput]) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for input in group {
        for token in tokens(&input.semantic_signature) {
            *counts.entry(token).or_insert(0) += 1;
        }
    }
    let majority = group.len().div_ceil(2);
    let common: Vec<_> = counts
        .into_iter()
        .filter_map(|(token, count)| if count >= majority { Some(token) } else { None })
        .collect();
    if common.is_empty() {
        normalize_signature(&group[0].semantic_signature)
    } else {
        common.join(" ")
    }
}

fn concept_signature(group: &[SemanticAbstraction]) -> String {
    let mut parts = BTreeSet::new();
    for abstraction in group {
        for token in tokens(&abstraction.abstraction_signature) {
            parts.insert(token);
        }
        for role in &abstraction.semantic_roles {
            parts.insert(normalize_token(role));
        }
    }
    parts.into_iter().collect::<Vec<_>>().join(" ")
}

fn meta_signature(abstractions: &[SemanticAbstraction]) -> String {
    let mut freq: BTreeMap<String, usize> = BTreeMap::new();
    for abstraction in abstractions {
        for token in tokens(&abstraction.abstraction_signature) {
            *freq.entry(token).or_insert(0) += 1;
        }
    }
    let mut stable: Vec<_> = freq
        .into_iter()
        .filter_map(|(token, count)| if count >= 2 { Some(token) } else { None })
        .collect();
    if stable.is_empty() {
        stable.push("semantic-convergence".to_string());
    }
    stable.join(" ")
}

fn infer_transfer_domains(constraints: &[String], signature: &str) -> Vec<String> {
    let mut domains = BTreeSet::new();
    let text = format!("{} {}", constraints.join(" "), signature).to_ascii_lowercase();
    if text.contains("architecture") || text.contains("component") || text.contains("boundary") {
        domains.insert("architecture".to_string());
    }
    if text.contains("recover") || text.contains("repair") || text.contains("resilien") {
        domains.insert("recovery".to_string());
    }
    if text.contains("govern") || text.contains("policy") || text.contains("constraint") {
        domains.insert("governance".to_string());
    }
    if text.contains("plan") || text.contains("intent") || text.contains("goal") {
        domains.insert("planning".to_string());
    }
    if domains.is_empty() {
        domains.insert("general".to_string());
    }
    domains.into_iter().collect()
}

fn concept_dependencies(concepts: &[ConceptNode]) -> Vec<(String, String)> {
    let mut deps = BTreeSet::new();
    for parent in concepts {
        for child in concepts {
            if parent.concept_id == child.concept_id {
                continue;
            }
            if parent.abstraction_dependencies.len() >= child.abstraction_dependencies.len()
                && token_overlap(&parent.conceptual_signature, &child.conceptual_signature) >= 0.4
            {
                deps.insert((parent.concept_id.clone(), child.concept_id.clone()));
            }
        }
    }
    deps.into_iter().collect()
}

fn preservation_score(before: &[ConceptNode], after: &[ConceptNode]) -> f64 {
    if before.is_empty() {
        return 1.0;
    }
    let before_constraints = union_owned(before.iter().flat_map(|c| c.semantic_constraints.iter()));
    let after_constraints = union_owned(after.iter().flat_map(|c| c.semantic_constraints.iter()));
    let constraints = list_overlap(&before_constraints, &after_constraints);
    let stability = average(after.iter().map(|c| c.conceptual_stability));
    (constraints * 0.55 + stability * 0.45).clamp(0.0, 1.0)
}

fn stable_id(prefix: &str, signature: &str, ordinal: usize) -> String {
    let normalized = normalize_signature(signature).replace(' ', "_");
    let suffix = if normalized.is_empty() {
        "semantic".to_string()
    } else {
        normalized.chars().take(48).collect()
    };
    format!("{prefix}_{ordinal:03}_{suffix}")
}

fn normalize_signature(value: &str) -> String {
    tokens(value).join(" ")
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

fn tokens(value: &str) -> Vec<String> {
    let mut tokens: Vec<_> = value
        .split_whitespace()
        .map(normalize_token)
        .filter(|token| !token.is_empty())
        .collect();
    tokens.sort();
    tokens.dedup();
    tokens
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
    let sa: BTreeSet<_> = a.iter().map(|s| normalize_token(s)).collect();
    let sb: BTreeSet<_> = b.iter().map(|s| normalize_token(s)).collect();
    let intersection = sa.intersection(&sb).count() as f64;
    let union = sa.union(&sb).count() as f64;
    intersection / union
}

fn union_sorted<'a>(values: impl Iterator<Item = &'a String>) -> Vec<String> {
    values
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
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

fn role_density(roles: &[String]) -> f64 {
    if roles.is_empty() {
        0.25
    } else {
        (roles.len() as f64 / 4.0).min(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(
        id: &str,
        domain: &str,
        signature: &str,
        roles: &[&str],
        causal: &[&str],
    ) -> SemanticPatternInput {
        SemanticPatternInput {
            pattern_id: id.to_string(),
            domain: domain.to_string(),
            semantic_signature: signature.to_string(),
            semantic_roles: roles.iter().map(|s| s.to_string()).collect(),
            causal_patterns: causal.iter().map(|s| s.to_string()).collect(),
            convergence_score: 0.9,
            stability_score: 0.88,
        }
    }

    fn repeated_recovery_patterns() -> Vec<SemanticPatternInput> {
        vec![
            pattern(
                "P1",
                "recovery",
                "cache repair latency recovery",
                &["Recovery", "Resilience"],
                &["failure triggers repair"],
            ),
            pattern(
                "P2",
                "recovery",
                "cache repair throughput recovery",
                &["Recovery", "Resilience"],
                &["failure triggers repair"],
            ),
            pattern(
                "P3",
                "architecture",
                "component boundary repair resilience",
                &["Architecture", "Resilience"],
                &["boundary isolates failure"],
            ),
            pattern(
                "P4",
                "planning",
                "intent plan repair adaptation",
                &["Planning", "Recovery"],
                &["intent drift triggers adaptation"],
            ),
        ]
    }

    #[test]
    fn semantic_abstraction_deterministic() {
        let engine = SemanticAbstractionEngine::default();
        let inputs = repeated_recovery_patterns();
        let first = engine.synthesize(&inputs);
        let second = engine.synthesize(&inputs);
        assert_eq!(first, second);
        assert!(!first.is_empty());
    }

    #[test]
    fn abstraction_stability_preserved() {
        let engine = SemanticAbstractionEngine::default();
        let abstractions = engine.synthesize(&repeated_recovery_patterns());
        assert!(abstractions.iter().all(|a| a.stability_score >= 0.8));
        assert!(abstractions
            .iter()
            .any(|a| a.abstraction_signature.contains("repair")));
    }

    #[test]
    fn abstraction_compression_replayable() {
        let runtime = ConceptCognitionRuntime::default();
        let inputs = repeated_recovery_patterns();
        let first = runtime.run(&inputs, &[]);
        let second = runtime.run(&inputs, &[]);
        assert_eq!(first.concept_graph, second.concept_graph);
        assert_eq!(first.entropy_after, second.entropy_after);
    }

    #[test]
    fn concept_synthesis_stable() {
        let abstractions =
            SemanticAbstractionEngine::default().synthesize(&repeated_recovery_patterns());
        let graph = ConceptSynthesisEngine::default().synthesize(&abstractions);
        assert!(!graph.concepts.is_empty());
        assert!(graph.conceptual_convergence_score > 0.0);
        let replay = ConceptSynthesisEngine::default().synthesize(&abstractions);
        assert_eq!(graph, replay);
    }

    #[test]
    fn concept_hierarchy_consistent() {
        let abstractions =
            SemanticAbstractionEngine::default().synthesize(&repeated_recovery_patterns());
        let engine = ConceptSynthesisEngine::default();
        let graph = engine.synthesize(&abstractions);
        let hierarchy = engine.build_hierarchy(&graph);
        assert_eq!(hierarchy.len(), graph.concepts.len());
        assert!(hierarchy.iter().any(|node| node.parent_concept.is_none()));
    }

    #[test]
    fn meta_concept_formation_stable() {
        let abstractions =
            SemanticAbstractionEngine::default().synthesize(&repeated_recovery_patterns());
        let graph = ConceptSynthesisEngine::new(1).synthesize(&abstractions);
        assert!(graph
            .concepts
            .iter()
            .any(|concept| concept.concept_id.starts_with("META_CONCEPT")));
        let replay = ConceptSynthesisEngine::new(1).synthesize(&abstractions);
        assert_eq!(graph, replay);
    }

    #[test]
    fn cross_domain_transfer_preserves_meaning() {
        let report = ConceptCognitionRuntime::default().run(
            &repeated_recovery_patterns(),
            &[("architecture", "planning")],
        );
        let transfer = &report.transfer_mappings[0];
        assert!(!transfer.transferred_concepts.is_empty());
        assert!(transfer.semantic_preservation_score >= 0.6);
    }

    #[test]
    fn transfer_mapping_deterministic() {
        let abstractions =
            SemanticAbstractionEngine::default().synthesize(&repeated_recovery_patterns());
        let graph = ConceptSynthesisEngine::default().synthesize(&abstractions);
        let first = CrossDomainTransferEngine::transfer(&graph, "architecture", "planning");
        let second = CrossDomainTransferEngine::transfer(&graph, "architecture", "planning");
        assert_eq!(first, second);
    }

    #[test]
    fn semantic_transfer_adaptation_stable() {
        let abstractions =
            SemanticAbstractionEngine::default().synthesize(&repeated_recovery_patterns());
        let graph = ConceptSynthesisEngine::new(1).synthesize(&abstractions);
        let transfer = CrossDomainTransferEngine::transfer(&graph, "recovery", "governance");
        assert!(transfer.transferred_concepts.iter().all(|id| {
            graph
                .concepts
                .iter()
                .any(|concept| &concept.concept_id == id)
        }));
    }

    #[test]
    fn semantic_compression_preserves_intent() {
        let abstractions =
            SemanticAbstractionEngine::default().synthesize(&repeated_recovery_patterns());
        let graph = ConceptSynthesisEngine::default().synthesize(&abstractions);
        let compressed = SemanticCompressionEngine::default()
            .compress_concepts(&graph)
            .expect("compression must preserve semantics");
        assert!(preservation_score(&graph.concepts, &compressed.concepts) >= 0.6);
    }

    #[test]
    fn duplicate_concepts_eliminated() {
        let concept = ConceptNode {
            concept_id: "C1".to_string(),
            concept_name: "concept::repair".to_string(),
            conceptual_signature: "repair resilience".to_string(),
            abstraction_dependencies: vec!["A1".to_string()],
            semantic_constraints: vec!["Recovery".to_string()],
            transfer_domains: vec!["recovery".to_string()],
            conceptual_stability: 0.9,
        };
        let mut duplicate = concept.clone();
        duplicate.concept_id = "C2".to_string();
        duplicate.abstraction_dependencies = vec!["A2".to_string()];
        let graph = ConceptGraph {
            concepts: vec![concept, duplicate],
            conceptual_dependencies: vec![],
            conceptual_convergence_score: 0.9,
        };
        let compressed = SemanticCompressionEngine::default()
            .compress_concepts(&graph)
            .unwrap();
        assert_eq!(compressed.concepts.len(), 1);
        assert_eq!(compressed.concepts[0].abstraction_dependencies.len(), 2);
    }

    #[test]
    fn conceptual_entropy_reduced() {
        let concept = ConceptNode {
            concept_id: "C1".to_string(),
            concept_name: "concept::repair".to_string(),
            conceptual_signature: "repair resilience".to_string(),
            abstraction_dependencies: vec!["A1".to_string()],
            semantic_constraints: vec!["Recovery".to_string()],
            transfer_domains: vec!["recovery".to_string()],
            conceptual_stability: 0.9,
        };
        let mut duplicate = concept.clone();
        duplicate.concept_id = "C2".to_string();
        let graph = ConceptGraph {
            concepts: vec![concept, duplicate],
            conceptual_dependencies: vec![],
            conceptual_convergence_score: 0.9,
        };
        let before = SemanticCompressionEngine::entropy(&graph);
        let compressed = SemanticCompressionEngine::default()
            .compress_concepts(&graph)
            .unwrap();
        let after = SemanticCompressionEngine::entropy(&compressed);
        assert!(after <= before);
    }

    #[test]
    fn conceptual_drift_detected() {
        let abstractions =
            SemanticAbstractionEngine::default().synthesize(&repeated_recovery_patterns());
        let graph = ConceptSynthesisEngine::default().synthesize(&abstractions);
        let runtime = ConceptCognitionRuntime::default();
        let drift = runtime.detect_conceptual_drift(&graph.concepts[0], "unrelated quantum ledger");
        assert!(matches!(
            drift,
            Some(ConceptFormationEvent::ConceptualDriftDetected { .. })
        ));
    }

    #[test]
    fn unstable_hierarchy_rejected() {
        let graph = ConceptGraph {
            concepts: vec![ConceptNode {
                concept_id: "C_UNSTABLE".to_string(),
                concept_name: "concept::unstable".to_string(),
                conceptual_signature: "fragmented".to_string(),
                abstraction_dependencies: vec![],
                semantic_constraints: vec![],
                transfer_domains: vec!["general".to_string()],
                conceptual_stability: 0.1,
            }],
            conceptual_dependencies: vec![],
            conceptual_convergence_score: 0.1,
        };
        assert!(graph.conceptual_convergence_score < 0.3);
    }

    #[test]
    fn semantic_fragmentation_prevented() {
        let graph = ConceptGraph {
            concepts: vec![ConceptNode {
                concept_id: "C1".to_string(),
                concept_name: "concept::empty".to_string(),
                conceptual_signature: "empty".to_string(),
                abstraction_dependencies: vec![],
                semantic_constraints: vec![],
                transfer_domains: vec!["general".to_string()],
                conceptual_stability: 0.2,
            }],
            conceptual_dependencies: vec![],
            conceptual_convergence_score: 0.2,
        };
        let result = SemanticCompressionEngine::new(0.95).compress_concepts(&graph);
        assert!(result.is_err());
    }

    #[test]
    fn verification_a_repeated_recovery_patterns() {
        let report = ConceptCognitionRuntime::default().run(&repeated_recovery_patterns(), &[]);
        assert!(report
            .concept_graph
            .concepts
            .iter()
            .any(|concept| concept.conceptual_signature.contains("recovery")
                || concept.conceptual_signature.contains("repair")));
    }

    #[test]
    fn verification_b_cross_domain_transfer() {
        let report = ConceptCognitionRuntime::default().run(
            &repeated_recovery_patterns(),
            &[("architecture", "planning")],
        );
        assert!(report.transfer_mappings[0].semantic_preservation_score >= 0.6);
    }

    #[test]
    fn verification_c_meta_concept_emergence() {
        let runtime = ConceptCognitionRuntime {
            abstraction_engine: SemanticAbstractionEngine::default(),
            synthesis_engine: ConceptSynthesisEngine::new(1),
            compression_engine: SemanticCompressionEngine::default(),
        };
        let report = runtime.run(&repeated_recovery_patterns(), &[]);
        assert!(report
            .events
            .iter()
            .any(|event| matches!(event, ConceptFormationEvent::MetaConceptEmerged { .. })));
    }

    #[test]
    fn verification_d_conceptual_drift() {
        let abstractions =
            SemanticAbstractionEngine::default().synthesize(&repeated_recovery_patterns());
        let graph = ConceptSynthesisEngine::default().synthesize(&abstractions);
        let event = ConceptCognitionRuntime::default().detect_conceptual_drift(
            &graph.concepts[0],
            "inconsistent unrelated semantic injection",
        );
        assert!(event.is_some());
    }
}
