use std::collections::{BTreeMap, BTreeSet};
use std::sync::{OnceLock, RwLock};

use architecture_ir::stable_v03::{
    ArchitectureGraph, ArchitectureGraphBuilder, Edge, Node, NodeType, RelationType,
};
use serde_json::Value;
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
pub struct SearchInput {
    pub intent: IntentState,
    pub recall: Option<RecallContext>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArchitectureCandidate {
    pub id: String,
    pub architecture: ArchitectureGraph,
    pub score: f64,
    pub depth: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Intent {
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum SemanticRelationType {
    DerivedFrom,
    DependsOn,
    SimilarTo,
    ConstraintHint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relation {
    pub from: usize,
    pub to: usize,
    pub relation_type: SemanticRelationType,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SemanticRepresentation {
    pub relations: Vec<Relation>,
    pub intents: Vec<Intent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Goal {
    pub target: String,
    pub required_intents: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub beam_width: usize,
    pub max_depth: usize,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReasoningInput {
    pub semantic: SemanticRepresentation,
    pub context: Context,
    pub goal: Goal,
    pub recall: Option<RecallContext>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub architecture: ArchitectureGraph,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hypothesis {
    pub id: usize,
    pub state: State,
    pub score: f32,
    pub parent: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SearchScoreBreakdown {
    pub relevance: f32,
    pub goal_distance: f32,
    pub constraint_score: f32,
    pub memory_match: f32,
    pub total: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EvaluationScore {
    pub goal_distance: f32,
    pub constraint_violations: usize,
    pub structural_consistency: f32,
    pub reusability: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Accept,
    Reject,
    Continue,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TraceStep {
    pub stage: &'static str,
    pub hypothesis_id: Option<usize>,
    pub depth: usize,
    pub beam_width: usize,
    pub candidates: usize,
    pub pruned: usize,
    pub recall_hits: usize,
    pub detail: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TraceStats {
    pub total_nodes: usize,
    pub max_depth: usize,
    pub recall_hit_rate: f32,
    pub avg_branching: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReasoningTrace {
    pub steps: Vec<TraceStep>,
    pub generated_hypotheses: usize,
    pub search_depth: usize,
    pub recall_hit_rate: f32,
    pub execution_time_ms: u128,
    pub relations: Vec<Relation>,
    pub decisions: Vec<(usize, Decision)>,
    pub score_breakdown: BTreeMap<usize, SearchScoreBreakdown>,
    pub evaluation_breakdown: BTreeMap<usize, EvaluationScore>,
    pub frontier_by_depth: BTreeMap<usize, Vec<usize>>,
    pub stats: TraceStats,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReasoningResult {
    pub solution: ArchitectureCandidate,
    pub candidates: Vec<ArchitectureCandidate>,
    pub confidence: f32,
    pub trace: ReasoningTrace,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReasoningSearchResult {
    pub candidates: Vec<ArchitectureCandidate>,
    pub trace: ReasoningTrace,
}

pub trait ReasoningCore {
    fn reason(&self, input: ReasoningInput) -> ReasoningResult;
}

pub trait DesignSearchEngine: Send + Sync {
    fn search(&self, input: SearchInput) -> Vec<ArchitectureCandidate> {
        self.search_with_trace(input).candidates
    }

    fn search_with_trace(&self, input: SearchInput) -> ReasoningSearchResult {
        let candidates = self.search(input);
        ReasoningSearchResult {
            candidates,
            trace: ReasoningTrace::default(),
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
    fn search(&self, input: SearchInput) -> Vec<ArchitectureCandidate> {
        self.search_with_trace(input).candidates
    }

    fn search_with_trace(&self, input: SearchInput) -> ReasoningSearchResult {
        let semantic = SemanticAdapter::adapt(&input.intent, input.recall.as_ref());
        let goal = Goal {
            target: input.intent.raw.clone(),
            required_intents: semantic
                .intents
                .iter()
                .map(|intent| intent.label.clone())
                .collect(),
        };
        let result = self.reason(ReasoningInput {
            semantic,
            context: Context {
                beam_width: self.beam_width,
                max_depth: self.max_depth,
                timeout_ms: 2_000,
            },
            goal,
            recall: input.recall,
        });
        ReasoningSearchResult {
            candidates: result.candidates,
            trace: result.trace,
        }
    }
}

impl ReasoningCore for DeterministicBeamSearchEngine {
    fn reason(&self, input: ReasoningInput) -> ReasoningResult {
        let mut trace = ReasoningTrace::default();
        trace.relations = input.semantic.relations.clone();
        trace.steps.push(TraceStep {
            stage: "semantic_adapter",
            depth: 0,
            beam_width: self.beam_width,
            candidates: input.semantic.intents.len(),
            detail: format!(
                "intents={} relations={}",
                input.semantic.intents.len(),
                trace.relations.len()
            ),
            ..TraceStep::default()
        });

        let recall_seeds = seed_hypotheses(&input, &mut trace, self.beam_width);
        trace.recall_hit_rate = if recall_seeds.is_empty() { 0.0 } else { 1.0 };
        trace.stats.recall_hit_rate = trace.recall_hit_rate;

        let mut next_hypothesis_id = recall_seeds
            .iter()
            .map(|hypothesis| hypothesis.id)
            .max()
            .unwrap_or(0)
            + 1;
        let mut beam = if recall_seeds.is_empty() {
            vec![Hypothesis {
                id: 0,
                state: State {
                    architecture: ArchitectureGraph::default(),
                },
                score: 0.0,
                parent: None,
            }]
        } else {
            recall_seeds
        };
        let mut all = beam.clone();
        let mut best_score_seen = beam.iter().map(|candidate| candidate.score).fold(0.0, f32::max);
        let mut expanded_parents = 0usize;
        let mut produced_children = 0usize;

        trace.generated_hypotheses = beam.len();
        trace.frontier_by_depth.insert(0, beam.iter().map(|hypothesis| hypothesis.id).collect());

        for depth in 1..=input.context.max_depth {
            let mut candidates = Vec::new();
            for parent in &beam {
                let expansions = expand_hypothesis(parent, &input, depth, &mut next_hypothesis_id, &mut trace);
                if !expansions.is_empty() {
                    expanded_parents += 1;
                    produced_children += expansions.len();
                }
                candidates.extend(expansions);
            }

            if candidates.is_empty() {
                break;
            }

            let entropy = score_entropy(&candidates);
            let effective_beam_width = self.adaptive_beam_width(depth, entropy);
            let recall_hits = candidates
                .iter()
                .filter(|hypothesis| {
                    trace.score_breakdown
                        .get(&hypothesis.id)
                        .map(|score| score.memory_match > 0.0)
                        .unwrap_or(false)
                })
                .count();
            let (pruned, pruned_count) = prune_candidates(candidates, &input, effective_beam_width);
            trace.steps.push(TraceStep {
                stage: "search",
                depth,
                beam_width: effective_beam_width,
                candidates: pruned.len() + pruned_count,
                pruned: pruned_count,
                recall_hits,
                detail: format!("entropy={entropy:.4}"),
                ..TraceStep::default()
            });

            if pruned.is_empty() {
                break;
            }

            let depth_best = pruned.iter().map(|candidate| candidate.score).fold(0.0, f32::max);
            trace.search_depth = depth;
            trace.frontier_by_depth.insert(depth, pruned.iter().map(|hypothesis| hypothesis.id).collect());
            trace.generated_hypotheses += pruned.len();
            all.extend(pruned.clone());
            beam = pruned;

            if should_terminate_early(depth_best, best_score_seen, &beam, &input, self) {
                trace.steps.push(TraceStep {
                    stage: "early_termination",
                    depth,
                    beam_width: effective_beam_width,
                    candidates: beam.len(),
                    pruned: 0,
                    recall_hits,
                    detail: format!("best_score={depth_best:.3}"),
                    ..TraceStep::default()
                });
                break;
            }

            best_score_seen = best_score_seen.max(depth_best);
        }

        trace.stats.total_nodes = all.len();
        trace.stats.max_depth = trace.search_depth;
        trace.stats.avg_branching = if expanded_parents == 0 {
            0.0
        } else {
            produced_children as f32 / expanded_parents as f32
        };
        trace.execution_time_ms = synthetic_latency_ms(&trace);

        let best = select_best_hypothesis(&all, &mut trace, &input);
        let candidates = hypotheses_to_candidates(&all, &trace);

        ReasoningResult {
            solution: ArchitectureCandidate {
                id: candidate_id(&best.state.architecture, best.id, hypothesis_depth(best.id, &trace)),
                architecture: best.state.architecture.clone(),
                score: f64::from(best.score),
                depth: hypothesis_depth(best.id, &trace),
            },
            candidates,
            confidence: best.score,
            trace,
        }
    }
}

struct SemanticAdapter;

impl SemanticAdapter {
    fn adapt(intent: &IntentState, recall: Option<&RecallContext>) -> SemanticRepresentation {
        if let Some(cached) = semantic_cache()
            .read()
            .expect("semantic cache read lock")
            .get(&intent.raw)
            .cloned()
        {
            return cached;
        }

        let mut labels = intent
            .tokens
            .iter()
            .filter(|token| !token.trim().is_empty())
            .map(|token| token.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        if let Some(recall) = recall {
            for pattern in &recall.patterns {
                for tag in &pattern.tags {
                    labels.insert(tag.to_ascii_lowercase());
                }
            }
        }

        let semantic = if labels.is_empty() {
            strict_json_fallback(&intent.raw).unwrap_or_else(default_semantic_representation)
        } else {
            semantic_from_labels(labels.into_iter().collect())
        };

        semantic_cache()
            .write()
            .expect("semantic cache write lock")
            .insert(intent.raw.clone(), semantic.clone());
        semantic
    }
}

fn semantic_cache() -> &'static RwLock<BTreeMap<String, SemanticRepresentation>> {
    static CACHE: OnceLock<RwLock<BTreeMap<String, SemanticRepresentation>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(BTreeMap::new()))
}

fn llm_cache() -> &'static RwLock<BTreeMap<String, SemanticRepresentation>> {
    static CACHE: OnceLock<RwLock<BTreeMap<String, SemanticRepresentation>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(BTreeMap::new()))
}

fn strict_json_fallback(raw: &str) -> Option<SemanticRepresentation> {
    if let Some(cached) = llm_cache()
        .read()
        .expect("llm cache read lock")
        .get(raw)
        .cloned()
    {
        return Some(cached);
    }
    let parsed = serde_json::from_str::<Value>(raw).ok()?;
    let intents = parsed.get("intents")?.as_array()?;
    let relations = parsed
        .get("relations")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let intents = intents
        .iter()
        .map(|value| Some(Intent { label: value.as_str()?.to_string() }))
        .collect::<Option<Vec<_>>>()?;
    let relations = relations
        .iter()
        .map(parse_relation)
        .collect::<Option<Vec<_>>>()?;
    let semantic = SemanticRepresentation { relations, intents };
    llm_cache()
        .write()
        .expect("llm cache write lock")
        .insert(raw.to_string(), semantic.clone());
    Some(semantic)
}

fn parse_relation(value: &Value) -> Option<Relation> {
    let relation_type = match value.get("type")?.as_str()? {
        "DerivedFrom" => SemanticRelationType::DerivedFrom,
        "DependsOn" => SemanticRelationType::DependsOn,
        "SimilarTo" => SemanticRelationType::SimilarTo,
        "ConstraintHint" => SemanticRelationType::ConstraintHint,
        _ => return None,
    };
    Some(Relation {
        from: value.get("from")?.as_u64()? as usize,
        to: value.get("to")?.as_u64()? as usize,
        relation_type,
    })
}

fn default_semantic_representation() -> SemanticRepresentation {
    semantic_from_labels(vec!["general".to_string()])
}

fn semantic_from_labels(mut labels: Vec<String>) -> SemanticRepresentation {
    labels.sort();
    labels.dedup();
    let intents = labels
        .into_iter()
        .map(|label| Intent { label })
        .collect::<Vec<_>>();
    let relations = intents
        .windows(2)
        .enumerate()
        .map(|(index, _)| Relation {
            from: index,
            to: index + 1,
            relation_type: SemanticRelationType::DerivedFrom,
        })
        .collect();
    SemanticRepresentation { relations, intents }
}

fn seed_hypotheses(
    input: &ReasoningInput,
    trace: &mut ReasoningTrace,
    beam_width: usize,
) -> Vec<Hypothesis> {
    let Some(recall) = input.recall.as_ref() else {
        trace.steps.push(TraceStep {
            stage: "recall",
            depth: 0,
            beam_width,
            candidates: 0,
            detail: "recall_miss".to_string(),
            ..TraceStep::default()
        });
        return Vec::new();
    };

    let mut seeds = recall
        .patterns
        .iter()
        .enumerate()
        .map(|(index, pattern)| {
            let score = (pattern.score as f32).clamp(0.0, 1.0);
            trace.steps.push(TraceStep {
                stage: "recall",
                hypothesis_id: Some(index),
                depth: 0,
                beam_width,
                candidates: recall.patterns.len(),
                recall_hits: 1,
                detail: format!("pattern={} score={score:.3}", pattern.record_id),
                ..TraceStep::default()
            });
            Hypothesis {
                id: index,
                state: State {
                    architecture: pattern.architecture.clone(),
                },
                score,
                parent: None,
            }
        })
        .collect::<Vec<_>>();
    seeds.sort_by(|lhs, rhs| rhs.score.total_cmp(&lhs.score).then_with(|| lhs.id.cmp(&rhs.id)));
    seeds.truncate(beam_width.max(1));
    seeds
}

fn expand_hypothesis(
    parent: &Hypothesis,
    input: &ReasoningInput,
    depth: usize,
    next_hypothesis_id: &mut usize,
    trace: &mut ReasoningTrace,
) -> Vec<Hypothesis> {
    let tokens = candidate_tokens(input);
    let existing = parent
        .state
        .architecture
        .nodes()
        .iter()
        .map(|node| node.id.0.clone())
        .collect::<BTreeSet<_>>();

    let mut expansions = Vec::new();
    for token in tokens {
        if existing.contains(&token) {
            continue;
        }
        let candidate = build_child_hypothesis(parent, &token, *next_hypothesis_id);
        *next_hypothesis_id += 1;

        let breakdown = score_breakdown(&candidate, input);
        let evaluation = evaluate_hypothesis(&candidate, input);
        let decision = decide(&breakdown, &evaluation);
        trace.score_breakdown.insert(candidate.id, breakdown.clone());
        trace.evaluation_breakdown.insert(candidate.id, evaluation.clone());
        trace.decisions.push((candidate.id, decision.clone()));
        trace.steps.push(TraceStep {
            stage: "hypothesis_generation",
            hypothesis_id: Some(candidate.id),
            depth,
            beam_width: 0,
            candidates: 1,
            recall_hits: usize::from(breakdown.memory_match > 0.0),
            detail: format!("token={token} total={:.3} decision={decision:?}", breakdown.total),
            ..TraceStep::default()
        });
        if matches!(decision, Decision::Reject) {
            continue;
        }

        let mut accepted = candidate;
        accepted.score = breakdown.total;
        expansions.push(accepted);
    }
    expansions
}

fn build_child_hypothesis(parent: &Hypothesis, token: &str, id: usize) -> Hypothesis {
    let node = Node::new(token.to_string(), classify_token(token));
    let architecture = if parent.state.architecture.nodes().is_empty() {
        ArchitectureGraphBuilder::new()
            .add_node(node)
            .build()
            .unwrap_or_default()
    } else {
        let predecessor = parent
            .state
            .architecture
            .nodes()
            .last()
            .map(|existing| existing.id.clone())
            .expect("predecessor should exist");
        let mut builder = parent
            .state
            .architecture
            .nodes()
            .iter()
            .cloned()
            .fold(ArchitectureGraphBuilder::new(), |builder, node| builder.add_node(node));
        builder = parent
            .state
            .architecture
            .edges()
            .iter()
            .cloned()
            .fold(builder, |builder, edge| builder.add_edge(edge));
        builder
            .add_node(node.clone())
            .add_edge(Edge::new(predecessor, node.id.clone(), RelationType::DependsOn))
            .build()
            .unwrap_or_default()
    };

    Hypothesis {
        id,
        state: State { architecture },
        score: 0.0,
        parent: Some(parent.id),
    }
}

fn prune_candidates(
    candidates: Vec<Hypothesis>,
    input: &ReasoningInput,
    beam_width: usize,
) -> (Vec<Hypothesis>, usize) {
    let threshold = 0.15_f32;
    let mut ranked = candidates;
    ranked.sort_by(|lhs, rhs| {
        rhs.score
            .total_cmp(&lhs.score)
            .then_with(|| semantic_state_hash(&lhs.state.architecture).cmp(&semantic_state_hash(&rhs.state.architecture)))
            .then_with(|| lhs.id.cmp(&rhs.id))
    });

    let total = ranked.len();
    let mut seen = BTreeSet::new();
    let mut retained = Vec::new();
    for hypothesis in ranked {
        if hypothesis.score < threshold {
            continue;
        }
        let state_hash = state_hash(&hypothesis.state.architecture);
        let semantic_hash = semantic_state_hash(&hypothesis.state.architecture);
        if !seen.insert((state_hash, semantic_hash)) {
            continue;
        }
        retained.push(hypothesis);
        if retained.len() >= beam_width.max(1).min(input.context.beam_width.max(1) * 2) {
            break;
        }
    }
    let pruned = total.saturating_sub(retained.len());
    (retained, pruned)
}

fn select_best_hypothesis(
    hypotheses: &[Hypothesis],
    trace: &mut ReasoningTrace,
    input: &ReasoningInput,
) -> Hypothesis {
    let mut ranked = hypotheses.to_vec();
    ranked.sort_by(|lhs, rhs| {
        rhs.score
            .total_cmp(&lhs.score)
            .then_with(|| semantic_state_hash(&lhs.state.architecture).cmp(&semantic_state_hash(&rhs.state.architecture)))
            .then_with(|| lhs.id.cmp(&rhs.id))
    });
    let best = ranked.into_iter().next().unwrap_or(Hypothesis {
        id: 0,
        state: State {
            architecture: ArchitectureGraph::default(),
        },
        score: 0.0,
        parent: None,
    });
    trace
        .evaluation_breakdown
        .entry(best.id)
        .or_insert_with(|| evaluate_hypothesis(&best, input));
    trace
        .score_breakdown
        .entry(best.id)
        .or_insert_with(|| score_breakdown(&best, input));
    trace.decisions.push((best.id, Decision::Accept));
    trace.steps.push(TraceStep {
        stage: "decision",
        hypothesis_id: Some(best.id),
        depth: hypothesis_depth(best.id, trace),
        beam_width: 1,
        candidates: hypotheses.len(),
        detail: "selected_best".to_string(),
        ..TraceStep::default()
    });
    best
}

fn score_breakdown(hypothesis: &Hypothesis, input: &ReasoningInput) -> SearchScoreBreakdown {
    let weights = [0.35_f32, 0.25_f32, 0.20_f32, 0.20_f32];
    let relevance = relevance_score(hypothesis, input);
    let goal_similarity = goal_similarity(hypothesis, input);
    let goal_distance = (1.0 - goal_similarity).clamp(0.0, 1.0);
    let violations = constraint_violations(hypothesis, input) as f32;
    let constraint_score = (1.0 - (violations * 0.2)).clamp(0.0, 1.0);
    let memory_match = memory_match_score(hypothesis, input);
    let total = (weights[0] * relevance
        + weights[1] * (1.0 - goal_distance)
        + weights[2] * constraint_score
        + weights[3] * memory_match)
        .clamp(0.0, 1.0);
    SearchScoreBreakdown {
        relevance,
        goal_distance,
        constraint_score,
        memory_match,
        total,
    }
}

fn evaluate_hypothesis(hypothesis: &Hypothesis, input: &ReasoningInput) -> EvaluationScore {
    let validation = hypothesis.state.architecture.validate();
    let goal_distance = 1.0 - goal_similarity(hypothesis, input);
    let constraint_violations = validation.errors.len() + constraint_violations(hypothesis, input);
    EvaluationScore {
        goal_distance,
        constraint_violations,
        structural_consistency: if validation.is_valid() { 1.0 } else { 0.5 },
        reusability: memory_match_score(hypothesis, input),
    }
}

fn decide(score: &SearchScoreBreakdown, evaluation: &EvaluationScore) -> Decision {
    if evaluation.constraint_violations > 0 {
        return Decision::Reject;
    }
    if score.total >= 0.75 && evaluation.goal_distance <= 0.25 {
        Decision::Accept
    } else {
        Decision::Continue
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
    input: &ReasoningInput,
    engine: &DeterministicBeamSearchEngine,
) -> bool {
    if best_score < engine.early_termination_score {
        return false;
    }
    let Some(best) = beam
        .iter()
        .max_by(|lhs, rhs| rhs.score.total_cmp(&lhs.score).reverse().then_with(|| lhs.id.cmp(&rhs.id)))
    else {
        return false;
    };
    let evaluation = evaluate_hypothesis(best, input);
    evaluation.goal_distance <= engine.early_goal_distance
        && (best_score - previous_best).abs() <= 0.02
}

fn candidate_tokens(input: &ReasoningInput) -> Vec<String> {
    let mut tokens = input
        .semantic
        .intents
        .iter()
        .map(|intent| intent.label.clone())
        .collect::<BTreeSet<_>>();
    if let Some(recall) = input.recall.as_ref() {
        for pattern in &recall.patterns {
            for node in pattern.architecture.nodes() {
                tokens.insert(node.id.0.clone());
            }
            for tag in &pattern.tags {
                tokens.insert(tag.to_ascii_lowercase());
            }
        }
    }
    tokens.into_iter().collect()
}

fn relevance_score(hypothesis: &Hypothesis, input: &ReasoningInput) -> f32 {
    if input.semantic.intents.is_empty() {
        return 0.0;
    }
    let node_ids = hypothesis
        .state
        .architecture
        .nodes()
        .iter()
        .map(|node| node.id.0.as_str())
        .collect::<BTreeSet<_>>();
    let hits = input
        .semantic
        .intents
        .iter()
        .filter(|intent| node_ids.contains(intent.label.as_str()))
        .count() as f32;
    (hits / input.semantic.intents.len() as f32).clamp(0.0, 1.0)
}

fn goal_similarity(hypothesis: &Hypothesis, input: &ReasoningInput) -> f32 {
    if input.goal.required_intents.is_empty() {
        return 1.0;
    }
    let required = input
        .goal
        .required_intents
        .iter()
        .map(|intent| intent.as_str())
        .collect::<BTreeSet<_>>();
    let current = hypothesis
        .state
        .architecture
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

fn memory_match_score(hypothesis: &Hypothesis, input: &ReasoningInput) -> f32 {
    let Some(recall) = input.recall.as_ref() else {
        return 0.0;
    };
    if recall.patterns.is_empty() {
        return 0.0;
    }
    let candidate_signature = semantic_state_hash(&hypothesis.state.architecture);
    recall
        .patterns
        .iter()
        .map(|pattern| {
            let pattern_signature = semantic_state_hash(&pattern.architecture);
            let overlap = signature_overlap(&candidate_signature, &pattern_signature);
            (0.7 * overlap + 0.3 * pattern.score as f32).clamp(0.0, 1.0)
        })
        .fold(0.0_f32, f32::max)
}

fn constraint_violations(hypothesis: &Hypothesis, input: &ReasoningInput) -> usize {
    let Some(recall) = input.recall.as_ref() else {
        return 0;
    };
    let node_ids = hypothesis
        .state
        .architecture
        .nodes()
        .iter()
        .map(|node| node.id.0.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    recall
        .constraints
        .iter()
        .filter(|constraint| {
            let wanted = constraint.value.to_ascii_lowercase();
            !node_ids.contains(&wanted)
                && !input.goal.target.to_ascii_lowercase().contains(&wanted)
        })
        .count()
}

fn state_hash(graph: &ArchitectureGraph) -> String {
    let nodes = graph
        .nodes()
        .iter()
        .map(|node| format!("{}:{:?}", node.id.0.to_ascii_lowercase(), node.node_type))
        .collect::<Vec<_>>()
        .join("|");
    let edges = graph
        .edges()
        .iter()
        .map(|edge| {
            format!(
                "{}>{}>{:?}",
                edge.source.0.to_ascii_lowercase(),
                edge.target.0.to_ascii_lowercase(),
                edge.relation
            )
        })
        .collect::<Vec<_>>()
        .join("|");
    format!("{nodes}::{edges}")
}

fn semantic_state_hash(graph: &ArchitectureGraph) -> String {
    let mut items = graph
        .nodes()
        .iter()
        .map(|node| node.id.0.to_ascii_lowercase())
        .collect::<Vec<_>>();
    items.sort();
    items.join("|")
}

fn signature_overlap(lhs: &str, rhs: &str) -> f32 {
    let lhs = lhs
        .split('|')
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    let rhs = rhs
        .split('|')
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    if lhs.is_empty() || rhs.is_empty() {
        return 0.0;
    }
    let overlap = lhs.intersection(&rhs).count() as f32;
    let denom = lhs.len().max(rhs.len()) as f32;
    (overlap / denom).clamp(0.0, 1.0)
}

fn classify_token(token: &str) -> NodeType {
    match token {
        "api" | "gateway" | "frontend" => NodeType::Interface,
        "db" | "database" | "postgres" | "mysql" | "redis" => NodeType::DataStore,
        "service" | "worker" | "auth" | "queue" => NodeType::Service,
        other => NodeType::Custom(other.to_string()),
    }
}

fn hypotheses_to_candidates(
    hypotheses: &[Hypothesis],
    trace: &ReasoningTrace,
) -> Vec<ArchitectureCandidate> {
    let mut candidates = hypotheses
        .iter()
        .map(|hypothesis| ArchitectureCandidate {
            id: candidate_id(&hypothesis.state.architecture, hypothesis.id, hypothesis_depth(hypothesis.id, trace)),
            architecture: hypothesis.state.architecture.clone(),
            score: f64::from(hypothesis.score),
            depth: hypothesis_depth(hypothesis.id, trace),
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|lhs, rhs| rhs.score.total_cmp(&lhs.score).then_with(|| lhs.id.cmp(&rhs.id)));
    candidates
}

fn hypothesis_depth(id: usize, trace: &ReasoningTrace) -> usize {
    trace
        .frontier_by_depth
        .iter()
        .find_map(|(depth, ids)| ids.contains(&id).then_some(*depth))
        .unwrap_or(0)
}

fn candidate_id(graph: &ArchitectureGraph, hypothesis_id: usize, depth: usize) -> String {
    let signature = graph
        .nodes()
        .iter()
        .map(|node| node.id.0.as_str())
        .collect::<Vec<_>>()
        .join("-");
    format!("h{hypothesis_id}-d{depth}-{signature}")
}

fn synthetic_latency_ms(trace: &ReasoningTrace) -> u128 {
    let total = trace.stats.total_nodes as f32;
    let depth = trace.stats.max_depth as f32;
    let branching = trace.stats.avg_branching;
    ((total * 2.0) + (depth * 2.0) + branching).round() as u128
}

impl DeterministicBeamSearchEngine {
    pub fn adaptive_beam_width(&self, depth: usize, entropy: f32) -> usize {
        if !self.adaptive_beam {
            return self.beam_width.clamp(self.min_beam_width, self.max_beam_width);
        }
        let scale = (entropy * self.max_beam_width as f32).round() as isize;
        let width = self.beam_width as isize + scale - depth as isize;
        width
            .clamp(self.min_beam_width as isize, self.max_beam_width as isize)
            as usize
    }
}
