use std::collections::{BTreeMap, BTreeSet};

use architecture_ir::stable_v03::{
    ArchitectureGraph, ArchitectureGraphBuilder, Edge, Node, NodeType, RelationType,
};
pub use contracts::{
    Context, Decision, EvaluationScore, Goal, Hypothesis, HypothesisId, Intent, MemoryCandidate,
    MemoryRef, NodeId, ReasoningInput, ReasoningTrace, Relation, RelationId,
    RelationType as ContractRelationType, RequestId, ScoreParts, SemanticHash,
    SemanticRepresentation, State, StateHash, Strategy, StrategyReason, TraceProofStep, TraceStats,
    TraceStep, ValidationReason, ValidationResult, request_id_for, semantic_hash_for_text,
    state_hash_for_graph,
};
use bridge::reasoning_input_from_intent;
use world_model::stable_v03::IntentState;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Constraint {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecalledPattern {
    pub record_id: String,
    pub architecture: ArchitectureGraph,
    pub score: f64,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RecallContext {
    pub patterns: Vec<RecalledPattern>,
    pub constraints: Vec<Constraint>,
    pub confidence: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArchitectureCandidate {
    pub id: String,
    pub architecture: ArchitectureGraph,
    pub score: f64,
    pub depth: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReasoningResult {
    pub solution: ArchitectureCandidate,
    pub candidates: Vec<ArchitectureCandidate>,
    pub confidence: f32,
    pub trace: ReasoningTrace,
    pub validation: ValidationResult,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReasoningSearchResult {
    pub candidates: Vec<ArchitectureCandidate>,
    pub trace: ReasoningTrace,
    pub validation: ValidationResult,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HypothesisAuditSnapshot {
    pub hypotheses: Vec<Hypothesis>,
    pub trace: ReasoningTrace,
    pub validation: ValidationResult,
}

pub trait ReasoningCore {
    fn reason(&self, input: ReasoningInput) -> ReasoningResult;
}

pub trait DesignSearchEngine: Send + Sync {
    fn search(&self, input: ReasoningInput) -> Vec<ArchitectureCandidate> {
        self.search_with_trace(input).candidates
    }

    fn search_with_trace(&self, input: ReasoningInput) -> ReasoningSearchResult {
        let candidates = self.search(input);
        ReasoningSearchResult {
            candidates,
            trace: ReasoningTrace::new(RequestId::default(), Vec::new()),
            validation: ValidationResult::new(Vec::new()),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DeterministicBeamSearchEngine {
    pub beam_width: usize,
    pub max_depth: usize,
    pub min_beam_width: usize,
    pub max_beam_width: usize,
    pub adaptive_beam: bool,
    pub early_termination_score: f32,
    pub early_goal_distance: f32,
}

impl Default for DeterministicBeamSearchEngine {
    fn default() -> Self {
        Self {
            beam_width: 3,
            max_depth: 2,
            min_beam_width: 1,
            max_beam_width: 6,
            adaptive_beam: true,
            early_termination_score: 0.82,
            early_goal_distance: 0.18,
        }
    }
}

impl DesignSearchEngine for DeterministicBeamSearchEngine {
    fn search(&self, input: ReasoningInput) -> Vec<ArchitectureCandidate> {
        self.search_with_trace(input).candidates
    }

    fn search_with_trace(&self, input: ReasoningInput) -> ReasoningSearchResult {
        let result = self.reason(input);
        ReasoningSearchResult {
            candidates: result.candidates,
            trace: result.trace,
            validation: result.validation,
        }
    }
}

impl ReasoningCore for DeterministicBeamSearchEngine {
    fn reason(&self, reasoning_input: ReasoningInput) -> ReasoningResult {
        let request_id = reasoning_input.request_id.clone();

        let mut steps = Vec::new();
        let mut all = Vec::new();
        let mut next_id = 0usize;

        let mut beam = seed_hypotheses(&reasoning_input, &mut next_id);
        let recall_hits = beam.len();
        steps.push(TraceStep {
            depth: 0,
            beam_width: self.beam_width,
            candidates: beam.len().max(1),
            pruned: 0,
            recall_hits,
        });
        if beam.is_empty() {
            beam.push(empty_hypothesis(&reasoning_input.semantic.hash));
        }
        all.extend(beam.clone());

        let mut previous_best = beam.iter().map(|hypothesis| hypothesis.score).fold(0.0, f32::max);
        for depth in 1..=self.max_depth {
            let mut candidates = Vec::new();
            for parent in &beam {
                candidates.extend(expand_hypothesis(parent, &reasoning_input, depth, &mut next_id));
            }
            if candidates.is_empty() {
                break;
            }

            let entropy = score_entropy(&candidates);
            let beam_width = self.adaptive_beam_width(depth, entropy);
            candidates.sort_by(|lhs, rhs| {
                rhs.score
                    .total_cmp(&lhs.score)
                    .then_with(|| lhs.id.cmp(&rhs.id))
            });
            let total_candidates = candidates.len();
            let deduped = dedup_hypotheses(candidates);
            let pruned = total_candidates.saturating_sub(deduped.len());
            let next_beam = deduped
                .into_iter()
                .take(beam_width)
                .collect::<Vec<_>>();
            let recall_hits = next_beam
                .iter()
                .filter(|hypothesis| hypothesis.score_parts.memory > 0.0)
                .count();
            steps.push(TraceStep {
                depth,
                beam_width,
                candidates: total_candidates,
                pruned,
                recall_hits,
            });
            if next_beam.is_empty() {
                break;
            }
            let best_score = next_beam.iter().map(|hypothesis| hypothesis.score).fold(0.0, f32::max);
            all.extend(next_beam.clone());
            if should_terminate_early(best_score, previous_best, &next_beam, self) {
                break;
            }
            previous_best = best_score;
            all.sort_by(|lhs, rhs| rhs.score.total_cmp(&lhs.score).then_with(|| lhs.id.cmp(&rhs.id)));
            all.dedup_by(|lhs, rhs| lhs.state_hash == rhs.state_hash);
            all.sort_by(|lhs, rhs| lhs.depth.cmp(&rhs.depth).then_with(|| lhs.id.cmp(&rhs.id)));
            beam = next_beam;
        }

        let proof_steps = proof_steps_for(&all, &reasoning_input.memory_candidates);
        let trace = ReasoningTrace::with_explainability(
            request_id,
            steps,
            proof_steps,
            strategy_reason_for(self, &all),
        );
        let validation = validate_hypotheses(&all);
        let mut candidates = hypotheses_to_candidates(&all);
        candidates.sort_by(|lhs, rhs| {
            rhs.score
                .total_cmp(&lhs.score)
                .then_with(|| lhs.id.cmp(&rhs.id))
        });
        let solution = candidates.first().cloned().unwrap_or_else(|| ArchitectureCandidate {
            id: "empty".to_string(),
            architecture: ArchitectureGraph::default(),
            score: 0.0,
            depth: 0,
        });

        ReasoningResult {
            confidence: solution.score as f32,
            solution,
            candidates,
            trace,
            validation,
        }
    }
}

fn seed_hypotheses(
    reasoning_input: &ReasoningInput,
    next_id: &mut usize,
) -> Vec<Hypothesis> {
    let mut seeds = reasoning_input
        .semantic
        .intents
        .iter()
        .enumerate()
        .map(|(rank, intent)| {
            let architecture = ArchitectureGraphBuilder::new()
                .add_node(Node::new(intent.label.clone(), classify_token(&intent.label)))
                .build()
                .unwrap_or_default();
            let memory = memory_score_from_candidates(&reasoning_input.memory_candidates);
            let score_parts = ScoreParts {
                relevance: 1.0,
                goal_distance: 1.0,
                constraint: 1.0,
                memory,
            };
            let score = EvaluationScore::from_parts(score_parts.clone());
            Hypothesis {
                id: HypothesisId(*next_id + rank),
                state: State {
                    architecture: architecture.clone(),
                },
                parent: None,
                depth: 0,
                score: score.total,
                score_parts,
                state_hash: state_hash_for_graph(&architecture),
                semantic_hash: reasoning_input.semantic.hash.clone(),
            }
        })
        .collect::<Vec<_>>();
    *next_id += seeds.len();
    seeds.sort_by(|lhs, rhs| {
        rhs.score
            .total_cmp(&lhs.score)
            .then_with(|| lhs.id.cmp(&rhs.id))
    });
    seeds.truncate(reasoning_input.context.beam_width.max(1));
    seeds
}

fn expand_hypothesis(
    parent: &Hypothesis,
    input: &ReasoningInput,
    depth: usize,
    next_id: &mut usize,
) -> Vec<Hypothesis> {
    let tokens = candidate_tokens(input);
    let existing = parent
        .state
        .architecture
        .nodes()
        .iter()
        .map(|node| node.id.0.clone())
        .collect::<BTreeSet<_>>();
    let mut result = Vec::new();

    for token in tokens {
        if existing.contains(&token) {
            continue;
        }
        let architecture = append_token(&parent.state.architecture, &token);
        let score_parts = score_parts_for(&architecture, input);
        let score = EvaluationScore::from_parts(score_parts.clone());
        result.push(Hypothesis {
            id: HypothesisId(*next_id),
            state_hash: state_hash_for_graph(&architecture),
            semantic_hash: input.semantic.hash.clone(),
            state: State { architecture },
            parent: Some(parent.id),
            depth,
            score: score.total,
            score_parts,
        });
        *next_id += 1;
    }

    result
}

fn append_token(base: &ArchitectureGraph, token: &str) -> ArchitectureGraph {
    let node = Node::new(token.to_string(), classify_token(token));
    if base.nodes().is_empty() {
        return ArchitectureGraphBuilder::new()
            .add_node(node)
            .build()
            .unwrap_or_default();
    }
    let predecessor = base
        .nodes()
        .last()
        .map(|node| node.id.clone())
        .expect("predecessor");
    let mut builder = base
        .nodes()
        .iter()
        .cloned()
        .fold(ArchitectureGraphBuilder::new(), |builder, node| builder.add_node(node));
    builder = base
        .edges()
        .iter()
        .cloned()
        .fold(builder, |builder, edge| builder.add_edge(edge));
    builder
        .add_node(node.clone())
        .add_edge(Edge::new(predecessor, node.id.clone(), RelationType::DependsOn))
        .build()
        .unwrap_or_default()
}

fn candidate_tokens(input: &ReasoningInput) -> Vec<String> {
    input
        .semantic
        .intents
        .iter()
        .map(|intent| intent.label.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn score_parts_for(
    architecture: &ArchitectureGraph,
    input: &ReasoningInput,
) -> ScoreParts {
    let relevance = semantic_overlap(architecture, &input.semantic);
    let goal_distance = goal_similarity(architecture, &input.goal);
    let constraint = 1.0;
    let memory = memory_score_from_candidates(&input.memory_candidates);
    ScoreParts {
        relevance,
        goal_distance,
        constraint,
        memory,
    }
}

/// Compute a memory score in [0, 1] from recalled candidates.
///
/// Strategy: weighted average of the top-3 candidate scores with
/// diminishing weights [0.6, 0.3, 0.1] so the strongest recall
/// signal dominates while remaining candidates add a small boost.
/// Returns 0.0 when no candidates are present (backward-compatible).
fn memory_score_from_candidates(candidates: &[MemoryCandidate]) -> f32 {
    if candidates.is_empty() {
        return 0.0;
    }
    let weights: [f32; 3] = [0.6, 0.3, 0.1];
    let mut score = 0.0_f32;
    let mut weight_sum = 0.0_f32;
    for (i, candidate) in candidates.iter().take(3).enumerate() {
        score += weights[i] * candidate.score;
        weight_sum += weights[i];
    }
    if weight_sum > 0.0 {
        (score / weight_sum).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn semantic_overlap(architecture: &ArchitectureGraph, semantic: &SemanticRepresentation) -> f32 {
    if semantic.intents.is_empty() {
        return 0.0;
    }
    let nodes = architecture
        .nodes()
        .iter()
        .map(|node| node.id.0.as_str())
        .collect::<BTreeSet<_>>();
    let hits = semantic
        .intents
        .iter()
        .filter(|intent| nodes.contains(intent.label.as_str()))
        .count() as f32;
    (hits / semantic.intents.len() as f32).clamp(0.0, 1.0)
}

fn goal_similarity(architecture: &ArchitectureGraph, goal: &Goal) -> f32 {
    if goal.required_intents.is_empty() {
        return 1.0;
    }
    let required = goal
        .required_intents
        .iter()
        .map(|item| item.as_str())
        .collect::<BTreeSet<_>>();
    let current = architecture
        .nodes()
        .iter()
        .map(|node| node.id.0.as_str())
        .collect::<BTreeSet<_>>();
    let overlap = current.intersection(&required).count() as f32;
    let denom = current.len().max(required.len()) as f32;
    if denom == 0.0 {
        1.0
    } else {
        (overlap / denom).clamp(0.0, 1.0)
    }
}

fn dedup_hypotheses(hypotheses: Vec<Hypothesis>) -> Vec<Hypothesis> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for hypothesis in hypotheses {
        if seen.insert((hypothesis.state_hash.clone(), hypothesis.semantic_hash.clone())) {
            result.push(hypothesis);
        }
    }
    result.sort_by(|lhs, rhs| rhs.score.total_cmp(&lhs.score).then_with(|| lhs.id.cmp(&rhs.id)));
    result
}

fn validate_hypotheses(hypotheses: &[Hypothesis]) -> ValidationResult {
    let mut reasons = Vec::new();
    let mut seen_hashes = BTreeSet::new();
    for hypothesis in hypotheses {
        if !hypothesis.is_valid() {
            reasons.push(ValidationReason::InvalidScore);
        }
        if !seen_hashes.insert(hypothesis.state_hash.clone()) {
            reasons.push(ValidationReason::DuplicateState);
        }
        if hypothesis
            .state
            .architecture
            .edges()
            .iter()
            .any(|edge| edge.source == edge.target)
        {
            reasons.push(ValidationReason::SelfLoop);
        }
    }
    ValidationResult::new(reasons)
}

fn hypotheses_to_candidates(hypotheses: &[Hypothesis]) -> Vec<ArchitectureCandidate> {
    let mut candidates = hypotheses
        .iter()
        .map(|hypothesis| ArchitectureCandidate {
            id: format!("{}-{}", hypothesis.id.0, hypothesis.state_hash.0),
            architecture: hypothesis.state.architecture.clone(),
            score: f64::from(hypothesis.score),
            depth: hypothesis.depth,
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|lhs, rhs| rhs.score.total_cmp(&lhs.score).then_with(|| lhs.id.cmp(&rhs.id)));
    candidates
}

fn empty_hypothesis(semantic_hash: &SemanticHash) -> Hypothesis {
    Hypothesis {
        id: HypothesisId(0),
        state: State {
            architecture: ArchitectureGraph::default(),
        },
        parent: None,
        depth: 0,
        score: 0.0,
        score_parts: ScoreParts {
            relevance: 0.0,
            goal_distance: 0.0,
            constraint: 1.0,
            memory: 0.0,
        },
        state_hash: StateHash("empty".to_string()),
        semantic_hash: semantic_hash.clone(),
    }
}

fn score_entropy(candidates: &[Hypothesis]) -> f32 {
    if candidates.len() <= 1 {
        return 0.0;
    }
    let mean = candidates.iter().map(|candidate| candidate.score).sum::<f32>() / candidates.len() as f32;
    candidates
        .iter()
        .map(|candidate| {
            let delta = candidate.score - mean;
            delta * delta
        })
        .sum::<f32>()
        / candidates.len() as f32
}

fn should_terminate_early(
    best_score: f32,
    previous_best: f32,
    beam: &[Hypothesis],
    engine: &DeterministicBeamSearchEngine,
) -> bool {
    let Some(best) = beam
        .iter()
        .max_by(|lhs, rhs| rhs.score.total_cmp(&lhs.score).reverse().then_with(|| lhs.id.cmp(&rhs.id)))
    else {
        return false;
    };
    best_score >= engine.early_termination_score
        && best.score_parts.goal_distance >= (1.0 - engine.early_goal_distance)
        && (best_score - previous_best).abs() <= 0.02
}

fn classify_token(token: &str) -> NodeType {
    match token {
        "api" | "gateway" | "frontend" => NodeType::Interface,
        "db" | "database" | "postgres" | "mysql" | "redis" => NodeType::DataStore,
        "service" | "worker" | "auth" | "queue" => NodeType::Service,
        other => NodeType::Custom(other.to_string()),
    }
}
impl DeterministicBeamSearchEngine {
    pub fn inspect_hypotheses(&self, reasoning_input: ReasoningInput) -> HypothesisAuditSnapshot {
        let request_id = reasoning_input.request_id.clone();
        let mut steps = Vec::new();
        let mut all = Vec::new();
        let mut next_id = 0usize;

        let mut beam = seed_hypotheses(&reasoning_input, &mut next_id);
        let recall_hits = beam.len();
        steps.push(TraceStep {
            depth: 0,
            beam_width: self.beam_width,
            candidates: beam.len().max(1),
            pruned: 0,
            recall_hits,
        });
        if beam.is_empty() {
            beam.push(empty_hypothesis(&reasoning_input.semantic.hash));
        }
        all.extend(beam.clone());

        let mut previous_best = beam
            .iter()
            .map(|hypothesis| hypothesis.score)
            .fold(0.0, f32::max);
        for depth in 1..=self.max_depth {
            let mut candidates = Vec::new();
            for parent in &beam {
                candidates.extend(expand_hypothesis(parent, &reasoning_input, depth, &mut next_id));
            }
            if candidates.is_empty() {
                break;
            }

            let entropy = score_entropy(&candidates);
            let beam_width = self.adaptive_beam_width(depth, entropy);
            candidates.sort_by(|lhs, rhs| {
                rhs.score
                    .total_cmp(&lhs.score)
                    .then_with(|| lhs.id.cmp(&rhs.id))
            });
            let total_candidates = candidates.len();
            let deduped = dedup_hypotheses(candidates);
            let pruned = total_candidates.saturating_sub(deduped.len());
            let next_beam = deduped.into_iter().take(beam_width).collect::<Vec<_>>();
            let recall_hits = next_beam
                .iter()
                .filter(|hypothesis| hypothesis.score_parts.memory > 0.0)
                .count();
            steps.push(TraceStep {
                depth,
                beam_width,
                candidates: total_candidates,
                pruned,
                recall_hits,
            });
            if next_beam.is_empty() {
                break;
            }
            let best_score = next_beam
                .iter()
                .map(|hypothesis| hypothesis.score)
                .fold(0.0, f32::max);
            all.extend(next_beam.clone());
            if should_terminate_early(best_score, previous_best, &next_beam, self) {
                break;
            }
            previous_best = best_score;
            all.sort_by(|lhs, rhs| {
                rhs.score
                    .total_cmp(&lhs.score)
                    .then_with(|| lhs.id.cmp(&rhs.id))
            });
            all.dedup_by(|lhs, rhs| lhs.state_hash == rhs.state_hash);
            all.sort_by(|lhs, rhs| lhs.depth.cmp(&rhs.depth).then_with(|| lhs.id.cmp(&rhs.id)));
            beam = next_beam;
        }

        let proof_steps = proof_steps_for(&all, &reasoning_input.memory_candidates);
        let trace = ReasoningTrace::with_explainability(
            request_id,
            steps,
            proof_steps,
            strategy_reason_for(self, &all),
        );
        let validation = validate_hypotheses(&all);
        HypothesisAuditSnapshot {
            hypotheses: all,
            trace,
            validation,
        }
    }

    pub fn adaptive_beam_width(&self, depth: usize, entropy: f32) -> usize {
        if !self.adaptive_beam {
            return self.beam_width.clamp(self.min_beam_width, self.max_beam_width);
        }
        let scale = (entropy * self.max_beam_width as f32).round() as isize;
        let width = self.beam_width as isize + scale - depth as isize;
        width
            .clamp(self.min_beam_width as isize, self.max_beam_width as isize) as usize
    }
}

impl DeterministicBeamSearchEngine {
    pub fn contract_input(&self, intent: &IntentState, recall: Option<&RecallContext>) -> ReasoningInput {
        let extra_tokens = recall
            .map(|recall| {
                recall
                    .patterns
                    .iter()
                    .flat_map(|pattern| pattern.tags.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        reasoning_input_from_intent(
            intent,
            &extra_tokens,
            Context {
                beam_width: self.beam_width,
                max_depth: self.max_depth,
                timeout_ms: 2_000,
            },
        )
    }
}

pub fn evaluate_hypothesis(hypothesis: &Hypothesis) -> EvaluationScore {
    EvaluationScore::from_parts(hypothesis.score_parts.clone())
}

pub fn decide(score: &EvaluationScore) -> Decision {
    if score.total >= 0.8 {
        Decision::Accept
    } else if score.total <= 0.35 {
        Decision::Reject
    } else {
        Decision::Continue
    }
}

pub fn validate_hypothesis_set(hypotheses: &[Hypothesis]) -> ValidationResult {
    validate_hypotheses(hypotheses)
}

fn proof_steps_for(hypotheses: &[Hypothesis], memory_candidates: &[MemoryCandidate]) -> Vec<TraceProofStep> {
    let by_id = hypotheses
        .iter()
        .map(|hypothesis| (hypothesis.id, hypothesis))
        .collect::<BTreeMap<_, _>>();
    let mut proof_steps = hypotheses
        .iter()
        .filter_map(|hypothesis| {
            let output = hypothesis
                .state
                .architecture
                .edges()
                .last()
                .map(edge_to_relation)?;
            let inputs = hypothesis
                .parent
                .and_then(|parent_id| by_id.get(&parent_id))
                .and_then(|parent| parent.state.architecture.edges().last())
                .map(edge_to_relation)
                .into_iter()
                .collect::<Vec<_>>();
            Some(TraceProofStep {
                trace_id: hypothesis.id.0,
                inputs,
                output,
                rule: Some("beam_expand".to_string()),
                memory_refs: memory_refs_from_candidates(memory_candidates),
                strategy: Strategy::BeamSearch,
            })
        })
        .collect::<Vec<_>>();
    proof_steps.sort_by(|lhs, rhs| {
        lhs.output
            .relation_id()
            .cmp(&rhs.output.relation_id())
            .then_with(|| lhs.trace_id.cmp(&rhs.trace_id))
    });
    proof_steps
}

fn memory_refs_from_candidates(candidates: &[MemoryCandidate]) -> Vec<MemoryRef> {
    candidates
        .iter()
        .take(3)
        .map(|candidate| MemoryRef {
            experience_id: candidate.id.clone(),
            confidence: candidate.score.clamp(0.0, 1.0),
            contribution: contribution_for_rank(candidate.rank),
        })
        .collect()
}

fn contribution_for_rank(rank: usize) -> f32 {
    match rank {
        0 => 1.0,
        1 => 0.6,
        2 => 0.3,
        _ => 0.1,
    }
}

fn strategy_reason_for(
    engine: &DeterministicBeamSearchEngine,
    hypotheses: &[Hypothesis],
) -> StrategyReason {
    let branching_factor = hypotheses
        .iter()
        .filter(|hypothesis| hypothesis.depth > 0)
        .count()
        .max(1);
    let estimated_cost = (hypotheses.len() * engine.beam_width.max(1)) as f32;
    StrategyReason {
        strategy: Strategy::BeamSearch,
        estimated_cost,
        branching_factor,
        reason: format!(
            "BeamSearch selected because branching={} and beam_width={} keep exploration bounded",
            branching_factor, engine.beam_width
        ),
    }
}

fn edge_to_relation(edge: &Edge) -> Relation {
    Relation {
        from: NodeId(edge.source.0.clone()),
        to: NodeId(edge.target.0.clone()),
        relation_type: ContractRelationType::DependsOn,
    }
}
