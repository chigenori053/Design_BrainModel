use std::collections::BTreeSet;

use architecture_ir::stable_v03::ArchitectureGraph;

pub mod marker;

pub use marker::ContractType;

pub type MemoryId = String;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct RequestId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct RelationId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SemanticHash(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct StateHash(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct HypothesisId(pub usize);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct NodeId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MemorySource {
    Cache,
    Index,
    Exact,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryRef {
    pub experience_id: MemoryId,
    pub confidence: f32,
    pub contribution: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub beam_width: usize,
    pub max_depth: usize,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Goal {
    pub target: String,
    pub required_intents: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Intent {
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RelationType {
    DerivedFrom,
    DependsOn,
    SimilarTo,
    ConstraintHint,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Strategy {
    RecallFirst,
    #[default]
    BeamSearch,
    Backward,
    Forward,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StrategyReason {
    pub strategy: Strategy,
    pub estimated_cost: f32,
    pub branching_factor: usize,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Relation {
    pub from: NodeId,
    pub to: NodeId,
    pub relation_type: RelationType,
}

impl Relation {
    pub fn relation_id(&self) -> RelationId {
        RelationId(format!(
            "{}>{}:{:?}",
            self.from.0, self.to.0, self.relation_type
        ))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SemanticRepresentation {
    pub relations: Vec<Relation>,
    pub intents: Vec<Intent>,
    pub hash: SemanticHash,
}

impl SemanticRepresentation {
    pub fn new(mut relations: Vec<Relation>, mut intents: Vec<Intent>) -> Self {
        relations.sort();
        intents.sort();
        let relations = relations
            .into_iter()
            .filter(|relation| relation.from != relation.to)
            .collect::<Vec<_>>();
        let intents = intents
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let hash = SemanticHash(stable_hash(
            &intents
                .iter()
                .map(|intent| intent.label.as_str())
                .chain(relations.iter().map(|relation| match relation.relation_type {
                    RelationType::DerivedFrom => "DerivedFrom",
                    RelationType::DependsOn => "DependsOn",
                    RelationType::SimilarTo => "SimilarTo",
                    RelationType::ConstraintHint => "ConstraintHint",
                }))
                .collect::<Vec<_>>()
                .join("|"),
        ));
        Self {
            relations,
            intents,
            hash,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReasoningInput {
    pub semantic: SemanticRepresentation,
    pub context: Context,
    pub goal: Goal,
    pub request_id: RequestId,
    /// Memory candidates recalled for this query.
    /// Scores are in [0,1]; empty when memory has no relevant records.
    pub memory_candidates: Vec<MemoryCandidate>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MemoryCandidate {
    pub id: MemoryId,
    pub score: f32,
    pub source: MemorySource,
    pub rank: usize,
}

impl MemoryCandidate {
    pub fn is_valid(&self) -> bool {
        (0.0..=1.0).contains(&self.score)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScoreParts {
    pub relevance: f32,
    pub goal_distance: f32,
    pub constraint: f32,
    pub memory: f32,
}

impl ScoreParts {
    pub fn is_valid(&self) -> bool {
        [self.relevance, self.goal_distance, self.constraint, self.memory]
            .into_iter()
            .all(|value| (0.0..=1.0).contains(&value))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EvaluationScore {
    pub total: f32,
    pub parts: ScoreParts,
}

impl EvaluationScore {
    pub const WEIGHTS: [f32; 4] = [0.35, 0.25, 0.20, 0.20];

    pub fn from_parts(parts: ScoreParts) -> Self {
        let total = (Self::WEIGHTS[0] * parts.relevance
            + Self::WEIGHTS[1] * parts.goal_distance
            + Self::WEIGHTS[2] * parts.constraint
            + Self::WEIGHTS[3] * parts.memory)
            .clamp(0.0, 1.0);
        Self { total, parts }
    }

    pub fn is_valid(&self) -> bool {
        self.parts.is_valid() && (0.0..=1.0).contains(&self.total)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct State {
    pub architecture: ArchitectureGraph,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hypothesis {
    pub id: HypothesisId,
    pub state: State,
    pub parent: Option<HypothesisId>,
    pub depth: usize,
    pub score: f32,
    pub score_parts: ScoreParts,
    pub state_hash: StateHash,
    pub semantic_hash: SemanticHash,
}

impl Hypothesis {
    pub fn is_valid(&self) -> bool {
        (0.0..=1.0).contains(&self.score)
            && self.score_parts.is_valid()
            && !self.state_hash.0.is_empty()
            && !self.semantic_hash.0.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Accept,
    Reject,
    Continue,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValidationReason {
    SelfLoop,
    DuplicateState,
    CycleDetected,
    InvalidScore,
    InvalidRelation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub reasons: Vec<ValidationReason>,
}

impl ValidationResult {
    pub fn new(reasons: Vec<ValidationReason>) -> Self {
        Self {
            is_valid: reasons.is_empty(),
            reasons,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TraceStep {
    pub depth: usize,
    pub beam_width: usize,
    pub candidates: usize,
    pub pruned: usize,
    pub recall_hits: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraceProofStep {
    pub trace_id: usize,
    pub inputs: Vec<Relation>,
    pub output: Relation,
    pub rule: Option<String>,
    pub memory_refs: Vec<MemoryRef>,
    pub strategy: Strategy,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TraceStats {
    pub total_nodes: usize,
    pub max_depth: usize,
    pub recall_hit_rate: f32,
    pub avg_branching: f32,
}

impl TraceStats {
    pub fn from_steps(steps: &[TraceStep]) -> Self {
        if steps.is_empty() {
            return Self::default();
        }
        let total_nodes = steps
            .iter()
            .map(|step| step.candidates.saturating_sub(step.pruned))
            .sum();
        let max_depth = steps.iter().map(|step| step.depth).max().unwrap_or(0);
        let total_candidates = steps.iter().map(|step| step.candidates).sum::<usize>();
        let recall_hits = steps.iter().map(|step| step.recall_hits).sum::<usize>();
        let branching_steps = steps.iter().filter(|step| step.depth > 0).count();
        let avg_branching = if branching_steps == 0 {
            0.0
        } else {
            steps.iter()
                .filter(|step| step.depth > 0)
                .map(|step| step.candidates as f32)
                .sum::<f32>()
                / branching_steps as f32
        };
        Self {
            total_nodes,
            max_depth,
            recall_hit_rate: if total_candidates == 0 {
                0.0
            } else {
                (recall_hits as f32 / total_candidates as f32).clamp(0.0, 1.0)
            },
            avg_branching,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReasoningTrace {
    pub request_id: RequestId,
    pub steps: Vec<TraceStep>,
    pub stats: TraceStats,
    pub proof_steps: Vec<TraceProofStep>,
    pub strategy_reason: StrategyReason,
}

impl ContractType for ReasoningInput {}
impl ContractType for SemanticRepresentation {}
impl ContractType for MemoryCandidate {}
impl ContractType for Hypothesis {}
impl ContractType for Relation {}
impl ContractType for EvaluationScore {}
impl ContractType for Decision {}
impl ContractType for ValidationResult {}
impl ContractType for ReasoningTrace {}
impl ContractType for StrategyReason {}
impl ContractType for TraceStep {}
impl ContractType for TraceStats {}

impl ReasoningTrace {
    pub fn new(request_id: RequestId, steps: Vec<TraceStep>) -> Self {
        Self::with_explainability(
            request_id,
            steps,
            Vec::new(),
            StrategyReason {
                strategy: Strategy::BeamSearch,
                estimated_cost: 0.0,
                branching_factor: 0,
                reason: "default beam search".to_string(),
            },
        )
    }

    pub fn with_proof_steps(
        request_id: RequestId,
        steps: Vec<TraceStep>,
        proof_steps: Vec<TraceProofStep>,
    ) -> Self {
        let stats = TraceStats::from_steps(&steps);
        let branching_factor = stats.avg_branching.round() as usize;
        Self::with_explainability(
            request_id,
            steps,
            proof_steps,
            StrategyReason {
                strategy: Strategy::BeamSearch,
                estimated_cost: stats.total_nodes as f32,
                branching_factor,
                reason: format!("beam search selected for branching={branching_factor}"),
            },
        )
    }

    pub fn with_explainability(
        request_id: RequestId,
        mut steps: Vec<TraceStep>,
        mut proof_steps: Vec<TraceProofStep>,
        strategy_reason: StrategyReason,
    ) -> Self {
        steps.sort_by(|lhs, rhs| {
            lhs.depth
                .cmp(&rhs.depth)
                .then_with(|| lhs.candidates.cmp(&rhs.candidates))
                .then_with(|| lhs.pruned.cmp(&rhs.pruned))
        });
        proof_steps.sort_by(|lhs, rhs| {
            lhs.output
                .relation_id()
                .cmp(&rhs.output.relation_id())
                .then_with(|| lhs.trace_id.cmp(&rhs.trace_id))
        });
        let stats = TraceStats::from_steps(&steps);
        Self {
            request_id,
            steps,
            stats,
            proof_steps,
            strategy_reason,
        }
    }
}

pub fn request_id_for(goal: &str, semantic_hash: &SemanticHash) -> RequestId {
    RequestId(stable_hash(&format!("{goal}::{}", semantic_hash.0)))
}

pub fn semantic_hash_for_text(text: &str) -> SemanticHash {
    SemanticHash(stable_hash(text))
}

pub fn state_hash_for_graph(graph: &ArchitectureGraph) -> StateHash {
    let nodes = graph
        .nodes()
        .iter()
        .map(|node| node.id.0.as_str())
        .collect::<Vec<_>>()
        .join("|");
    let edges = graph
        .edges()
        .iter()
        .map(|edge| format!("{}>{}", edge.source.0, edge.target.0))
        .collect::<Vec<_>>()
        .join("|");
    StateHash(stable_hash(&format!("{nodes}::{edges}")))
}

pub fn stable_hash(value: &str) -> String {
    let mut hash = 1469598103934665603_u64;
    for byte in value.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("{hash:016x}")
}
