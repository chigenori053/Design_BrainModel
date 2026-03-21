use std::collections::{HashMap, HashSet};

use design_search_engine::stable_v03::{
    Goal, MemoryRef, ReasoningTrace, Relation, RelationId, StrategyReason, TraceProofStep,
};

use crate::intent_refiner::{CoreSlot, IntentTrace, SlotSource};
use crate::stable_v03::RuntimeResult;

#[derive(Clone, Debug, PartialEq)]
pub struct Explanation {
    pub intent: Vec<SlotExplanation>,
    pub decisions: Vec<DecisionExplanation>,
    pub reasoning: Option<ReasoningExplanation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlotExplanation {
    pub slot: String,
    pub value: String,
    pub source: SlotSource,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionExplanation {
    pub message: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraceIndex {
    pub by_output: HashMap<RelationId, Vec<usize>>,
    by_trace_id: HashMap<usize, TraceProofStep>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProofNode {
    pub relation: Relation,
    pub rule: Option<String>,
    pub parents: Vec<ProofNode>,
    pub memory_refs: Vec<MemoryRef>,
    pub confidence: f32,
    pub cycle: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReasoningExplanation {
    pub goal: Option<Goal>,
    pub proof: ProofNode,
    pub compressed_proof: ProofNode,
    pub strategy_reason: StrategyReason,
    pub memory_summary: Vec<MemoryRef>,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompressionConfig {
    pub max_depth: usize,
    pub min_confidence: f32,
}

impl Default for CompressionConfig {
    fn default() -> Self {
        Self {
            max_depth: 3,
            min_confidence: 0.1,
        }
    }
}

pub trait ExplanationBuilder: Send + Sync {
    fn build(&self, trace: &IntentTrace, result: &RuntimeResult) -> Explanation;
}

#[derive(Clone, Debug, Default)]
pub struct DefaultExplanationBuilder;

impl ExplanationBuilder for DefaultExplanationBuilder {
    fn build(&self, trace: &IntentTrace, result: &RuntimeResult) -> Explanation {
        Explanation {
            intent: explain_slots(trace),
            decisions: explain_decisions(trace),
            reasoning: result.reasoning_trace.as_ref().and_then(explain_reasoning_trace),
        }
    }
}

impl TraceIndex {
    pub fn from_trace(trace: &ReasoningTrace) -> Self {
        let mut by_output = HashMap::<RelationId, Vec<usize>>::new();
        let mut by_trace_id = HashMap::new();
        for step in &trace.proof_steps {
            by_output
                .entry(step.output.relation_id())
                .or_default()
                .push(step.trace_id);
            by_trace_id.insert(step.trace_id, step.clone());
        }
        for ids in by_output.values_mut() {
            ids.sort();
        }
        Self {
            by_output,
            by_trace_id,
        }
    }

    pub fn trace_for(&self, relation: &Relation) -> Option<&TraceProofStep> {
        self.by_output
            .get(&relation.relation_id())
            .and_then(|ids| ids.first())
            .and_then(|trace_id| self.by_trace_id.get(trace_id))
    }
}

pub fn build_proof(target: &Relation, trace_index: &TraceIndex) -> ProofNode {
    build_proof_inner(target, trace_index, &mut HashSet::new())
}

fn build_proof_inner(
    target: &Relation,
    trace_index: &TraceIndex,
    visited: &mut HashSet<RelationId>,
) -> ProofNode {
    let relation_id = target.relation_id();
    if !visited.insert(relation_id.clone()) {
        return ProofNode {
            relation: target.clone(),
            rule: Some("cycle_detected".to_string()),
            parents: Vec::new(),
            memory_refs: Vec::new(),
            confidence: 0.0,
            cycle: true,
        };
    }

    let node = if let Some(step) = trace_index.trace_for(target) {
        let parents = step
            .inputs
            .iter()
            .map(|relation| build_proof_inner(relation, trace_index, visited))
            .collect::<Vec<_>>();
        let confidence = proof_confidence(step, &parents);
        ProofNode {
            relation: target.clone(),
            rule: step.rule.clone(),
            parents,
            confidence,
            memory_refs: step.memory_refs.clone(),
            cycle: false,
        }
    } else {
        ProofNode {
            relation: target.clone(),
            rule: None,
            parents: Vec::new(),
            memory_refs: Vec::new(),
            confidence: 1.0,
            cycle: false,
        }
    };
    visited.remove(&relation_id);
    node
}

pub fn infer_with_explain(trace: &ReasoningTrace) -> Option<(Vec<Relation>, ReasoningExplanation)> {
    infer_with_explain_full(trace)
}

pub fn infer_with_explain_full(
    trace: &ReasoningTrace,
) -> Option<(Vec<Relation>, ReasoningExplanation)> {
    let outputs = trace
        .proof_steps
        .iter()
        .map(|step| step.output.clone())
        .collect::<Vec<_>>();
    let explanation = explain_reasoning_trace(trace)?;
    Some((outputs, explanation))
}

pub fn explain_reasoning_trace(trace: &ReasoningTrace) -> Option<ReasoningExplanation> {
    let target = trace.proof_steps.last()?.output.clone();
    let trace_index = TraceIndex::from_trace(trace);
    let proof = build_proof(&target, &trace_index);
    let compressed_proof = compress_proof(proof.clone(), CompressionConfig::default());
    let memory_summary = summarize_memory(trace);
    let text = explain_text(&compressed_proof, &memory_summary, &trace.strategy_reason);
    Some(ReasoningExplanation {
        goal: None,
        proof,
        compressed_proof,
        strategy_reason: trace.strategy_reason.clone(),
        memory_summary,
        text,
    })
}

pub fn compress_proof(proof: ProofNode, config: CompressionConfig) -> ProofNode {
    compress_proof_inner(proof, 0, &config)
}

fn compress_proof_inner(proof: ProofNode, depth: usize, config: &CompressionConfig) -> ProofNode {
    let mut parents = proof
        .parents
        .into_iter()
        .filter(|parent| parent.confidence >= config.min_confidence || parent.cycle)
        .map(|parent| compress_proof_inner(parent, depth + 1, config))
        .collect::<Vec<_>>();
    if depth >= config.max_depth {
        parents.clear();
    }
    let mut collapsed = ProofNode {
        relation: proof.relation,
        rule: proof.rule,
        parents,
        memory_refs: proof.memory_refs,
        confidence: proof.confidence,
        cycle: proof.cycle,
    };
    while collapsed.parents.len() == 1
        && collapsed.memory_refs.is_empty()
        && collapsed.rule == collapsed.parents[0].rule
        && !collapsed.parents[0].cycle
    {
        let child = collapsed.parents.remove(0);
        collapsed.parents = child.parents;
    }
    collapsed
}

pub fn explain_text(
    proof: &ProofNode,
    memory_summary: &[MemoryRef],
    strategy_reason: &StrategyReason,
) -> String {
    let mut reasons = Vec::new();
    collect_reasons(proof, &mut reasons);
    let mut lines = vec![format!(
        "{} is connected to {}.",
        proof.relation.from.0, proof.relation.to.0
    )];
    if !reasons.is_empty() {
        lines.push(String::new());
        lines.push("Reasons:".to_string());
        for (index, reason) in reasons.iter().enumerate() {
            lines.push(format!("{}. {}", index + 1, reason));
        }
    }
    if !memory_summary.is_empty() {
        lines.push(String::new());
        lines.push("This reasoning is based on the following experiences:".to_string());
        for memory in memory_summary {
            lines.push(format!(
                "- {} (confidence: {:.2}, contribution: {:.2})",
                memory.experience_id, memory.confidence, memory.contribution
            ));
        }
    }
    lines.push(String::new());
    lines.push(format!(
        "Strategy:\n{:?} ({})",
        strategy_reason.strategy, strategy_reason.reason
    ));
    lines.join("\n")
}

fn collect_reasons(node: &ProofNode, reasons: &mut Vec<String>) {
    for parent in &node.parents {
        collect_reasons(parent, reasons);
        reasons.push(format!(
            "{} depends on {}",
            parent.relation.from.0, parent.relation.to.0
        ));
    }
    for memory in &node.memory_refs {
        reasons.push(format!("experience {} influenced this step", memory.experience_id));
    }
    if node.cycle {
        reasons.push(format!(
            "cycle detected at {} -> {}",
            node.relation.from.0, node.relation.to.0
        ));
    }
}

fn proof_confidence(step: &TraceProofStep, parents: &[ProofNode]) -> f32 {
    let memory_confidence = if step.memory_refs.is_empty() {
        1.0
    } else {
        step.memory_refs
            .iter()
            .map(|memory| memory.confidence * memory.contribution)
            .fold(0.0_f32, f32::max)
            .clamp(0.0, 1.0)
    };
    let parent_confidence = if parents.is_empty() {
        1.0
    } else {
        parents
            .iter()
            .map(|parent| parent.confidence)
            .sum::<f32>()
            / parents.len() as f32
    };
    ((memory_confidence + parent_confidence) / 2.0).clamp(0.0, 1.0)
}

fn summarize_memory(trace: &ReasoningTrace) -> Vec<MemoryRef> {
    let mut summary = trace
        .proof_steps
        .iter()
        .flat_map(|step| step.memory_refs.iter().cloned())
        .collect::<Vec<_>>();
    summary.sort_by(|lhs, rhs| {
        rhs.confidence
            .total_cmp(&lhs.confidence)
            .then_with(|| rhs.contribution.total_cmp(&lhs.contribution))
            .then_with(|| lhs.experience_id.cmp(&rhs.experience_id))
    });
    summary.dedup_by(|lhs, rhs| lhs.experience_id == rhs.experience_id);
    summary
}

pub fn explain_slots(trace: &IntentTrace) -> Vec<SlotExplanation> {
    let mut items = trace
        .final_slots
        .core
        .iter()
        .map(|(slot, value)| SlotExplanation {
            slot: format!("{slot:?}"),
            value: value.value.clone(),
            source: value.source,
        })
        .collect::<Vec<_>>();
    items.sort_by(|lhs, rhs| {
        lhs.slot
            .cmp(&rhs.slot)
            .then_with(|| lhs.value.cmp(&rhs.value))
    });
    items
}

pub fn explain_decisions(trace: &IntentTrace) -> Vec<DecisionExplanation> {
    let mut decisions = Vec::new();

    if trace.inferred.core.contains_key(&CoreSlot::InterfaceType) {
        decisions.push(DecisionExplanation {
            message: "Interface inferred from keyword 'api'".to_string(),
        });
    }
    if let Some(value) = trace.final_slots.core.get(&CoreSlot::Framework) {
        if value.source == SlotSource::Default {
            decisions.push(DecisionExplanation {
                message: format!(
                    "Framework defaulted from language '{}'",
                    trace
                        .final_slots
                        .core
                        .get(&CoreSlot::Language)
                        .map(|slot| slot.value.as_str())
                        .unwrap_or("unknown")
                ),
            });
        }
    }

    decisions
}

pub fn source_to_message(source: &SlotSource) -> &'static str {
    match source {
        SlotSource::Explicit => "explicitly specified",
        SlotSource::Inferred => "inferred from input",
        SlotSource::Memory => "derived from previous context",
        SlotSource::Default => "default applied",
    }
}
