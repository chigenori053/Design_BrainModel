use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use core_types::ObjectiveVector;
use dhm::Dhm;
use field_engine::{FieldEngine, TargetField};
use language_dhm::{LangId, LanguageDhm, LanguageUnit};
use meaning_extractor::MeaningExtractor;
use memory_space::{DesignState, InterferenceMode, MemoryInterferenceTelemetry};
use memory_store::{FileStore, InMemoryStore};
use recomposer::{MultiConceptInput, RecommendationInput, Recomposer, ResonanceReport};
use semantic_dhm::{ConceptUnit, SemanticDhm};

pub use chm::Chm;
pub use recomposer::ActionType;
pub use semantic_dhm::ConceptId;
pub use shm::{DesignRule, EffectVector, RuleCategory, RuleId, Shm, Transformation};

pub trait Evaluator {
    fn evaluate(&self, state: &DesignState) -> ObjectiveVector;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionMode {
    RecallFirst,
    ComputeFirst,
}

#[derive(Clone, Debug)]
pub struct ExecutionContext {
    pub request_id: u64,
    pub mode: ExecutionMode,
    pub depth: usize,
}

impl ExecutionContext {
    pub fn new(mode: ExecutionMode, depth: usize) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        Self {
            request_id: nanos ^ (std::process::id() as u64),
            mode,
            depth,
        }
    }
}

#[derive(Clone, Debug)]
pub struct HybridTraceRow {
    pub request_id: u64,
    pub depth: usize,
    pub mode: ExecutionMode,
    pub objective: ObjectiveVector,
}

pub struct HybridVM {
    evaluator: StructuralEvaluator,
    dhm: Dhm,
    language_dhm: LanguageDhm<FileStore<LangId, LanguageUnit>>,
    semantic_dhm: SemanticDhm<FileStore<ConceptId, ConceptUnit>>,
    meaning_extractor: MeaningExtractor,
    recomposer: Recomposer,
    mode: ExecutionMode,
    trace: Vec<HybridTraceRow>,
}

impl HybridVM {
    pub fn new(evaluator: StructuralEvaluator, dhm: Dhm, mode: ExecutionMode) -> Self {
        let language_dhm = Self::language_dhm_file(default_language_store_path())
            .expect("failed to initialize LanguageDHM");
        let semantic_dhm = Self::semantic_dhm_file(default_semantic_store_path())
            .expect("failed to initialize SemanticDHM");
        Self {
            evaluator,
            dhm,
            language_dhm,
            semantic_dhm,
            meaning_extractor: MeaningExtractor,
            recomposer: Recomposer,
            mode,
            trace: Vec::new(),
        }
    }

    pub fn with_default_memory(evaluator: StructuralEvaluator) -> Self {
        let path = default_store_path();
        let dhm = Dhm::open(path, memory_mode_from_env()).expect("failed to initialize DHM");
        Self::new(evaluator, dhm, ExecutionMode::RecallFirst)
    }

    pub fn mode(&self) -> ExecutionMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: ExecutionMode) {
        self.mode = mode;
    }

    pub fn evaluate(&mut self, state: &DesignState) -> ObjectiveVector {
        let depth = infer_depth_from_snapshot(&state.profile_snapshot);
        let ctx = ExecutionContext::new(self.mode, depth);
        self.evaluate_with_context(state, &ctx)
    }

    pub fn evaluate_with_context(
        &mut self,
        state: &DesignState,
        ctx: &ExecutionContext,
    ) -> ObjectiveVector {
        let base = self.evaluator.evaluate(state);
        let adjusted = match ctx.mode {
            ExecutionMode::RecallFirst => self.dhm.recall_first(&base),
            ExecutionMode::ComputeFirst => self.dhm.evaluate_with_recall(&base, ctx.depth),
        };
        self.trace.push(HybridTraceRow {
            request_id: ctx.request_id,
            depth: ctx.depth,
            mode: ctx.mode,
            objective: adjusted.clone(),
        });
        adjusted
    }

    pub fn take_memory_telemetry(&mut self) -> MemoryInterferenceTelemetry {
        self.dhm.telemetry()
    }

    pub fn take_trace(&mut self) -> Vec<HybridTraceRow> {
        std::mem::take(&mut self.trace)
    }

    pub fn analyze_text(&mut self, text: &str) -> Result<ConceptUnit, HybridVmError> {
        let embedding = embedding_from_text(text);
        let _ = self.language_dhm.insert(text, embedding.clone());
        let meaning = self.meaning_extractor.extract(text, &embedding);
        let concept_id = self.semantic_dhm.insert_meaning(&meaning);
        self.semantic_dhm
            .get(concept_id)
            .ok_or(HybridVmError::ConceptNotFound(concept_id))
    }

    pub fn get_concept(&self, id: ConceptId) -> Option<ConceptUnit> {
        self.semantic_dhm.get(id)
    }

    pub fn compare(
        &self,
        left: ConceptId,
        right: ConceptId,
    ) -> Result<ResonanceReport, HybridVmError> {
        let Some(c1) = self.semantic_dhm.get(left) else {
            return Err(HybridVmError::ConceptNotFound(left));
        };
        let Some(c2) = self.semantic_dhm.get(right) else {
            return Err(HybridVmError::ConceptNotFound(right));
        };
        let query = semantic_dhm::ConceptQuery {
            v: c1.v.clone(),
            a: c1.a,
            s: c1.s.clone(),
        };
        let score = semantic_dhm::resonance(&query, &c2, self.semantic_dhm.weights());
        let v_sim = dot_norm(&c1.v, &c2.v);
        let s_sim = dot_norm(&c1.s, &c2.s);
        let a_diff = (c1.a - c2.a).abs();

        Ok(ResonanceReport {
            c1: left,
            c2: right,
            score,
            v_sim,
            s_sim,
            a_diff,
        })
    }

    pub fn explain_multiple(
        &self,
        concept_ids: &[ConceptId],
    ) -> Result<recomposer::MultiExplanation, HybridVmError> {
        let ids = dedup_ids(concept_ids);
        if ids.len() < 2 {
            return Err(HybridVmError::InvalidInput(
                "multi explanation requires at least 2 unique concept ids",
            ));
        }
        let mut concepts = Vec::with_capacity(ids.len());
        for id in ids {
            let Some(c) = self.semantic_dhm.get(id) else {
                return Err(HybridVmError::ConceptNotFound(id));
            };
            concepts.push(c);
        }
        let input = MultiConceptInput {
            concepts,
            weights: None,
        };
        Ok(self
            .recomposer
            .explain_multiple(&input, &self.semantic_dhm.weights()))
    }

    pub fn recommend(
        &self,
        query_id: ConceptId,
        top_k: usize,
    ) -> Result<recomposer::RecommendationReport, HybridVmError> {
        let Some(query) = self.semantic_dhm.get(query_id) else {
            return Err(HybridVmError::ConceptNotFound(query_id));
        };
        let mut candidates = self
            .semantic_dhm
            .all_concepts()
            .into_iter()
            .filter(|c| c.id != query_id)
            .collect::<Vec<_>>();
        candidates.sort_by(|l, r| l.id.cmp(&r.id));

        let cap = candidates.len();
        let requested = top_k.max(1);
        let clamped_top_k = requested.min(cap);

        let input = RecommendationInput {
            query,
            candidates,
            top_k: clamped_top_k,
        };
        Ok(self
            .recomposer
            .recommend(&input, &self.semantic_dhm.weights()))
    }

    pub fn default_shm() -> Shm {
        Shm::with_default_rules()
    }

    pub fn empty_chm() -> Chm {
        Chm::default()
    }

    pub fn applicable_rules<'a>(shm: &'a Shm, state: &DesignState) -> Vec<&'a DesignRule> {
        shm.applicable_rules(state)
    }

    pub fn chm_insert_edge(chm: &mut Chm, from_rule: RuleId, to_rule: RuleId, strength: f64) {
        chm.insert_edge(from_rule, to_rule, strength);
    }

    pub fn chm_edge_count(chm: &Chm) -> usize {
        chm.edge_count()
    }

    pub fn rules(shm: &Shm) -> &[DesignRule] {
        shm.rules()
    }

    pub fn language_dhm_in_memory()
    -> std::io::Result<LanguageDhm<InMemoryStore<LangId, LanguageUnit>>> {
        LanguageDhm::in_memory()
    }

    pub fn language_dhm_file(
        path: impl AsRef<Path>,
    ) -> std::io::Result<LanguageDhm<FileStore<LangId, LanguageUnit>>> {
        LanguageDhm::file(path)
    }

    pub fn semantic_dhm_in_memory()
    -> std::io::Result<SemanticDhm<InMemoryStore<ConceptId, ConceptUnit>>> {
        SemanticDhm::in_memory()
    }

    pub fn semantic_dhm_file(
        path: impl AsRef<Path>,
    ) -> std::io::Result<SemanticDhm<FileStore<ConceptId, ConceptUnit>>> {
        SemanticDhm::file(path)
    }

    pub fn recomposer() -> Recomposer {
        Recomposer
    }

    pub fn for_cli_storage(base_dir: impl AsRef<Path>) -> io::Result<Self> {
        let base = base_dir.as_ref();
        std::fs::create_dir_all(base)?;
        let dhm = Dhm::open(base.join("dhm.bin"), memory_mode_from_env())?;
        let language_dhm = Self::language_dhm_file(base.join("language_dhm.bin"))?;
        let semantic_dhm = Self::semantic_dhm_file(base.join("semantic_dhm.bin"))?;
        Ok(Self {
            evaluator: StructuralEvaluator::default(),
            dhm,
            language_dhm,
            semantic_dhm,
            meaning_extractor: MeaningExtractor,
            recomposer: Recomposer,
            mode: ExecutionMode::RecallFirst,
            trace: Vec::new(),
        })
    }
}

#[derive(Debug)]
pub enum HybridVmError {
    Io(io::Error),
    ConceptNotFound(ConceptId),
    InvalidInput(&'static str),
}

impl std::fmt::Display for HybridVmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::ConceptNotFound(_) => write!(f, "Concept not found"),
            Self::InvalidInput(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for HybridVmError {}

impl From<io::Error> for HybridVmError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

fn infer_depth_from_snapshot(snapshot: &str) -> usize {
    let Some(raw) = snapshot.strip_prefix("history:") else {
        return 0;
    };
    raw.split(',').filter(|part| !part.is_empty()).count()
}

fn default_store_path() -> PathBuf {
    let id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("hybrid_vm_store_{}_{}.bin", std::process::id(), id))
}

fn default_language_store_path() -> PathBuf {
    std::env::temp_dir().join("hybrid_vm_language_dhm.bin")
}

fn default_semantic_store_path() -> PathBuf {
    std::env::temp_dir().join("hybrid_vm_semantic_dhm.bin")
}

fn embedding_from_text(text: &str) -> Vec<f32> {
    let mut out = vec![0.0f32; language_dhm::EMBEDDING_DIM];
    for (i, b) in text.bytes().enumerate() {
        let idx = (i.saturating_mul(131).saturating_add(b as usize)) % out.len();
        let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
        let value = (b as f32 / 255.0) - 0.5;
        out[idx] += sign * value;
    }
    out
}

fn dot_norm(a: &[f32], b: &[f32]) -> f32 {
    let an = normalize(a);
    let bn = normalize(b);
    an.iter().zip(bn.iter()).map(|(l, r)| l * r).sum::<f32>()
}

fn normalize(v: &[f32]) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm <= f32::EPSILON {
        return vec![0.0; v.len()];
    }
    v.iter().map(|x| x / norm).collect()
}

fn dedup_ids(ids: &[ConceptId]) -> Vec<ConceptId> {
    let mut out = Vec::new();
    for id in ids {
        if !out.contains(id) {
            out.push(*id);
        }
    }
    out
}

fn memory_mode_from_env() -> InterferenceMode {
    let raw = std::env::var("PHASE6_MEMORY_MODE").unwrap_or_else(|_| "v6.1".to_string());
    match raw.to_ascii_lowercase().as_str() {
        "off" | "disabled" | "a" => InterferenceMode::Disabled,
        "v6.0" | "v6_0" | "contractive" | "b" => InterferenceMode::Contractive,
        _ => InterferenceMode::Repulsive,
    }
}

#[derive(Clone, Debug)]
pub struct StructuralEvaluator {
    pub max_nodes: usize,
    pub max_edges: usize,
}

impl Default for StructuralEvaluator {
    fn default() -> Self {
        Self {
            max_nodes: 1000,
            max_edges: 5000,
        }
    }
}

impl StructuralEvaluator {
    pub fn new(max_nodes: usize, max_edges: usize) -> Self {
        Self {
            max_nodes,
            max_edges,
        }
    }
}

impl Evaluator for StructuralEvaluator {
    fn evaluate(&self, state: &DesignState) -> ObjectiveVector {
        let graph = &state.graph;
        let nodes = graph.nodes().len();
        let edges = graph.edges().len();

        let node_ratio = ratio(nodes, self.max_nodes);
        let max_possible_edges = nodes.saturating_mul(nodes.saturating_sub(1)) / 2;
        let edge_density = if max_possible_edges == 0 {
            0.0
        } else {
            ratio(edges, max_possible_edges)
        };

        let dag_penalty = if graph.is_dag() { 0.0 } else { 1.0 };
        let normalized_complexity =
            clamp01(0.45 * node_ratio + 0.45 * edge_density + 0.10 * dag_penalty);
        let degree_mass_entropy = graph.normalized_degree_mass_entropy();
        let degree_entropy = graph.normalized_degree_entropy();
        let field_base = if let Some(category_entropy) = graph.normalized_category_entropy() {
            0.75 * category_entropy + 0.25 * degree_mass_entropy
        } else {
            0.65 * degree_mass_entropy + 0.35 * degree_entropy
        };
        let f_field = clamp01(field_base.sqrt());

        let risk_raw = 0.25 * graph.normalized_degree_variance()
            + 0.20 * graph.normalized_max_degree()
            + 0.15 * graph.normalized_degree_gini()
            + 0.20 * edge_density
            + 0.20 * field_base;
        let f_risk = sigmoid(6.0 * (clamp01(risk_raw) - 0.5));
        let f_shape = if nodes < 3 {
            0.0
        } else {
            let clustering = graph.average_clustering_coefficient();
            clamp01(clustering)
        };

        ObjectiveVector {
            f_struct: 1.0 - normalized_complexity,
            f_field,
            f_risk,
            f_shape,
        }
        .clamped()
    }
}

pub struct FieldAwareEvaluator<'a> {
    pub structural: StructuralEvaluator,
    pub field_engine: &'a FieldEngine,
    pub target_field: &'a TargetField,
}

impl Evaluator for FieldAwareEvaluator<'_> {
    fn evaluate(&self, state: &DesignState) -> ObjectiveVector {
        let _ = self.field_engine;
        let _ = self.target_field;
        self.structural.evaluate(state)
    }
}

fn ratio(count: usize, max: usize) -> f64 {
    if max == 0 {
        return 1.0;
    }
    clamp01((count as f64) / (max as f64))
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

#[cfg(feature = "experimental")]
pub mod experimental {
    pub fn marker() -> &'static str {
        "experimental"
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use memory_space::{DesignNode, StructuralGraph, Uuid};

    use crate::{Evaluator, ExecutionContext, ExecutionMode, HybridVM, StructuralEvaluator};

    fn state_with_graph(nodes: usize, edges: &[(u128, u128)]) -> memory_space::DesignState {
        let mut graph = StructuralGraph::default();
        for i in 1..=nodes {
            graph = graph.with_node_added(DesignNode::new(
                Uuid::from_u128(i as u128),
                format!("N{i}"),
                BTreeMap::new(),
            ));
        }
        for (from, to) in edges {
            graph = graph.with_edge_added(Uuid::from_u128(*from), Uuid::from_u128(*to));
        }
        memory_space::DesignState::new(Uuid::from_u128(99), Arc::new(graph), "history:1,2")
    }

    #[test]
    fn supports_two_execution_modes() {
        let mut vm = HybridVM::with_default_memory(StructuralEvaluator::default());
        let s = state_with_graph(4, &[(1, 2), (2, 3)]);

        vm.set_mode(ExecutionMode::RecallFirst);
        let _a = vm.evaluate(&s);

        let ctx = ExecutionContext::new(ExecutionMode::ComputeFirst, 2);
        let _b = vm.evaluate_with_context(&s, &ctx);

        let trace = vm.take_trace();
        assert!(trace.len() >= 2);
    }

    #[test]
    fn structural_score_calculation_correctness() {
        let evaluator = StructuralEvaluator::new(10, 20);
        let simple = state_with_graph(2, &[]);
        let complex = state_with_graph(
            8,
            &[
                (1, 2),
                (1, 3),
                (2, 4),
                (3, 4),
                (4, 5),
                (5, 6),
                (6, 7),
                (7, 8),
            ],
        );

        let simple_obj = evaluator.evaluate(&simple);
        let complex_obj = evaluator.evaluate(&complex);
        assert!(simple_obj.f_struct > complex_obj.f_struct);
    }
}
