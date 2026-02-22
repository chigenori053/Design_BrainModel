use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use core_types::ObjectiveVector;
use design_reasoning::{
    HypothesisEngine, LanguageEngine, MeaningEngine, ProjectionEngine, SnapshotEngine,
};
use dhm::Dhm;
use field_engine::{FieldEngine, TargetField};
use language_dhm::{LangId, LanguageDhm, LanguageUnit};
use memory_space::{DesignState, MemoryInterferenceTelemetry};
use memory_store::{FileStore, InMemoryStore};
use recomposer::{
    DecisionReport, DesignReport, Recomposer, ResonanceReport,
};
use semantic_dhm::{ConceptUnit, SemanticDhm, SemanticL1Dhm, SemanticUnitL1};

mod ops;

pub use chm::Chm;
pub use design_reasoning::{DesignHypothesis, Explanation};
pub use recomposer::{ActionType, DecisionWeights, Recommendation};
pub use semantic_dhm::{
    ConceptId, DerivedRequirement, DesignProjection, L1Id, L2Config, L2Mode, MeaningLayerSnapshot,
    RequirementKind, RequirementRole as L1RequirementRole, SemanticError, SemanticUnitL1Input,
    Snapshotable,
};
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
    semantic_l1_dhm: SemanticL1Dhm<FileStore<L1Id, SemanticUnitL1>>,
    meaning_engine: MeaningEngine,
    projection_engine: ProjectionEngine,
    hypothesis_engine: HypothesisEngine,
    language_engine: LanguageEngine,
    snapshot_engine: SnapshotEngine,
    recomposer: Recomposer,
    mode: ExecutionMode,
    trace: Vec<HybridTraceRow>,
}

impl HybridVM {
    pub fn new(
        evaluator: StructuralEvaluator,
        dhm: Dhm,
        mode: ExecutionMode,
    ) -> Result<Self, SemanticError> {
        let language_dhm = Self::language_dhm_file(ops::util::default_language_store_path())
            .map_err(SemanticError::from)?;
        let semantic_dhm =
            Self::semantic_dhm_file(ops::util::default_semantic_store_path()).map_err(SemanticError::from)?;
        let semantic_l1_dhm =
            Self::semantic_l1_dhm_file(ops::util::default_l1_store_path()).map_err(SemanticError::from)?;
        Ok(Self {
            evaluator,
            dhm,
            language_dhm,
            semantic_dhm,
            semantic_l1_dhm,
            meaning_engine: MeaningEngine,
            projection_engine: ProjectionEngine,
            hypothesis_engine: HypothesisEngine,
            language_engine: LanguageEngine,
            snapshot_engine: SnapshotEngine,
            recomposer: Recomposer,
            mode,
            trace: Vec::new(),
        })
    }

    pub fn with_default_memory(evaluator: StructuralEvaluator) -> Result<Self, SemanticError> {
        let path = ops::util::default_store_path();
        let dhm = Dhm::open(path, ops::util::memory_mode_from_env()).map_err(SemanticError::from)?;
        Self::new(evaluator, dhm, ExecutionMode::RecallFirst)
    }

    pub fn mode(&self) -> ExecutionMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: ExecutionMode) {
        self.mode = mode;
    }

    pub fn evaluate(&mut self, state: &DesignState) -> ObjectiveVector {
        let depth = ops::util::infer_depth_from_snapshot(&state.profile_snapshot);
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

    pub fn analyze_text(&mut self, text: &str) -> Result<ConceptUnit, SemanticError> {
        ops::semantic::analyze_text(
            &self.meaning_engine,
            text,
            &mut self.language_dhm,
            &mut self.semantic_l1_dhm,
            &mut self.semantic_dhm,
        )
    }

    pub fn get_l1_unit(&self, id: L1Id) -> Option<SemanticUnitL1> {
        self.semantic_l1_dhm.get(id)
    }

    pub fn all_l1_units(&self) -> Vec<SemanticUnitL1> {
        self.semantic_l1_dhm.all_units()
    }

    pub fn remove_l1(&mut self, id: L1Id) -> Result<(), HybridVmError> {
        self.semantic_l1_dhm.remove(id).map_err(HybridVmError::Io)
    }

    pub fn rebuild_l2_from_l1(&mut self) -> Result<(), SemanticError> {
        ops::semantic::rebuild_l2_from_l1(&self.semantic_l1_dhm, &mut self.semantic_dhm)
    }

    pub fn rebuild_l2_from_l1_with_config(
        &mut self,
        config: L2Config,
    ) -> Result<(), SemanticError> {
        ops::semantic::rebuild_l2_from_l1_with_config(
            &self.semantic_l1_dhm,
            &mut self.semantic_dhm,
            config,
        )
    }

    pub fn rebuild_l2_from_l1_with_mode(&mut self, mode: L2Mode) -> Result<(), SemanticError> {
        ops::semantic::rebuild_l2_from_l1_with_mode(
            &self.semantic_l1_dhm,
            &mut self.semantic_dhm,
            mode,
        )
    }

    pub fn snapshot(&self) -> Result<MeaningLayerSnapshot, SemanticError> {
        ops::semantic::snapshot(&self.snapshot_engine, &self.semantic_l1_dhm, &self.semantic_dhm)
    }

    pub fn compare_snapshots(
        &self,
        left: &MeaningLayerSnapshot,
        right: &MeaningLayerSnapshot,
    ) -> Result<semantic_dhm::SnapshotDiff, SemanticError> {
        self.snapshot_engine.compare(left, right)
    }

    pub fn project_phase_a(&self) -> DesignProjection {
        ops::semantic::project_phase_a(&self.projection_engine, &self.semantic_l1_dhm, &self.semantic_dhm)
    }

    pub fn evaluate_hypothesis(
        &self,
        projection: &DesignProjection,
    ) -> Result<DesignHypothesis, SemanticError> {
        self.hypothesis_engine.evaluate_hypothesis(projection)
    }

    pub fn evaluate_design(&mut self, text: &str) -> Result<DesignHypothesis, SemanticError> {
        ops::semantic::evaluate_design(
            text,
            &self.meaning_engine,
            &self.projection_engine,
            &self.hypothesis_engine,
            &mut self.language_dhm,
            &mut self.semantic_l1_dhm,
            &mut self.semantic_dhm,
        )
    }

    pub fn explain_design(&mut self, text: &str) -> Result<Explanation, SemanticError> {
        ops::semantic::explain_design(
            text,
            &self.meaning_engine,
            &self.projection_engine,
            &self.hypothesis_engine,
            &self.language_engine,
            &mut self.language_dhm,
            &mut self.semantic_l1_dhm,
            &mut self.semantic_dhm,
        )
    }

    pub fn get_concept(&self, id: ConceptId) -> Option<ConceptUnit> {
        self.semantic_dhm.get(id)
    }

    pub fn compare(
        &self,
        left: ConceptId,
        right: ConceptId,
    ) -> Result<ResonanceReport, HybridVmError> {
        ops::recomposer::compare(&self.semantic_dhm, left, right)
    }

    pub fn explain_multiple(
        &self,
        concept_ids: &[ConceptId],
    ) -> Result<recomposer::MultiExplanation, HybridVmError> {
        ops::recomposer::explain_multiple(&self.semantic_dhm, &self.recomposer, concept_ids)
    }

    pub fn recommend(
        &self,
        query_id: ConceptId,
        top_k: usize,
    ) -> Result<recomposer::RecommendationReport, HybridVmError> {
        ops::recomposer::recommend(&self.semantic_dhm, &self.recomposer, query_id, top_k)
    }

    pub fn design_report(
        &self,
        concept_ids: &[ConceptId],
        top_k: usize,
    ) -> Result<DesignReport, HybridVmError> {
        ops::recomposer::design_report(&self.semantic_dhm, &self.recomposer, concept_ids, top_k)
    }

    pub fn decide(
        &self,
        ids: &[ConceptId],
        weights: DecisionWeights,
    ) -> Result<DecisionReport, HybridVmError> {
        ops::recomposer::decide(&self.semantic_dhm, &self.recomposer, ids, weights)
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

    pub fn semantic_l1_dhm_in_memory()
    -> std::io::Result<SemanticL1Dhm<InMemoryStore<L1Id, SemanticUnitL1>>> {
        SemanticL1Dhm::in_memory()
    }

    pub fn semantic_dhm_file(
        path: impl AsRef<Path>,
    ) -> std::io::Result<SemanticDhm<FileStore<ConceptId, ConceptUnit>>> {
        SemanticDhm::file(path)
    }

    pub fn semantic_l1_dhm_file(
        path: impl AsRef<Path>,
    ) -> std::io::Result<SemanticL1Dhm<FileStore<L1Id, SemanticUnitL1>>> {
        SemanticL1Dhm::file(path)
    }

    pub fn recomposer() -> Recomposer {
        Recomposer
    }

    pub fn for_cli_storage(base_dir: impl AsRef<Path>) -> io::Result<Self> {
        let base = base_dir.as_ref();
        std::fs::create_dir_all(base)?;
        let dhm = Dhm::open(base.join("dhm.bin"), ops::util::memory_mode_from_env())?;
        let language_dhm = Self::language_dhm_file(base.join("language_dhm.bin"))?;
        let semantic_dhm = Self::semantic_dhm_file(base.join("semantic_dhm.bin"))?;
        let semantic_l1_dhm = Self::semantic_l1_dhm_file(base.join("semantic_l1_dhm.bin"))?;
        Ok(Self {
            evaluator: StructuralEvaluator::default(),
            dhm,
            language_dhm,
            semantic_dhm,
            semantic_l1_dhm,
            meaning_engine: MeaningEngine,
            projection_engine: ProjectionEngine,
            hypothesis_engine: HypothesisEngine,
            language_engine: LanguageEngine,
            snapshot_engine: SnapshotEngine,
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
    Decision(recomposer::DecisionError),
}

impl std::fmt::Display for HybridVmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(err) => write!(f, "{err}"),
            Self::ConceptNotFound(_) => write!(f, "Concept not found"),
            Self::InvalidInput(msg) => write!(f, "{msg}"),
            Self::Decision(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for HybridVmError {}

impl From<io::Error> for HybridVmError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
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
    use design_reasoning::MeaningEngine;
    use std::collections::BTreeMap;
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use memory_space::{DesignNode, StructuralGraph, Uuid};
    use semantic_dhm::RequirementRole;

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
        let mut vm = HybridVM::with_default_memory(StructuralEvaluator::default()).expect("vm");
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

    #[test]
    fn analyze_text_creates_l1_and_l2_link() {
        let mut vm = HybridVM::with_default_memory(StructuralEvaluator::default()).expect("vm");
        let concept = vm
            .analyze_text("高速化したい。クラウド依存は避ける")
            .expect("analyze");
        assert!(!concept.l1_refs.is_empty());
        let all_l1 = vm.all_l1_units();
        assert!(all_l1.len() >= concept.l1_refs.len());
        let first = vm.get_l1_unit(concept.l1_refs[0]).expect("l1");
        assert!(!first.source_text.is_empty());
    }

    #[test]
    fn snapshot_matches_after_rebuild() {
        let store_dir = std::env::temp_dir().join(format!(
            "hybrid_vm_snapshot_test_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut vm = HybridVM::for_cli_storage(&store_dir).expect("vm");
        let _ = vm.analyze_text("security 강화");
        let before = vm.snapshot().expect("snapshot");
        vm.rebuild_l2_from_l1().expect("rebuild");
        let after = vm.snapshot().expect("snapshot");
        assert_eq!(before, after);
    }

    #[test]
    fn abstraction_v2_monotonic_examples() {
        let engine = MeaningEngine;
        let mem = engine.infer_abstraction("メモリは512MB以下");
        let fast_api = engine.infer_abstraction("高速なAPI");
        let high_perf = engine.infer_abstraction("高性能にしたい");
        let maybe_fast = engine.infer_abstraction("できるだけ速く");

        assert!(mem < fast_api);
        assert!(high_perf > mem);
        assert!(maybe_fast > 0.7);
    }

    #[test]
    fn polarity_depends_on_role_only() {
        let engine = MeaningEngine;
        assert_eq!(engine.infer_polarity(RequirementRole::Goal), 1);
        assert_eq!(engine.infer_polarity(RequirementRole::Optimization), 1);
        assert_eq!(engine.infer_polarity(RequirementRole::Constraint), -1);
        assert_eq!(engine.infer_polarity(RequirementRole::Prohibition), -1);
    }

    fn hypothesis_from_text(text: &str) -> crate::DesignHypothesis {
        let store_dir = std::env::temp_dir().join(format!(
            "hybrid_vm_hypothesis_test_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut vm = HybridVM::for_cli_storage(&store_dir).expect("vm");
        let _ = vm.analyze_text(text).expect("analyze");
        let projection = vm.project_phase_a();
        vm.evaluate_hypothesis(&projection)
            .expect("hypothesis evaluation should succeed")
    }

    #[test]
    fn hypothesis_score_direction_examples() {
        let perf = hypothesis_from_text("高速なAPI");
        let memory = hypothesis_from_text("メモリ512MB以下");
        assert!(perf.total_score > 0.0);
        assert!(memory.total_score < 0.0);
    }

    #[test]
    fn hypothesis_constraint_violation_baseline() {
        let memory = hypothesis_from_text("メモリ512MB以下");
        assert!(!memory.constraint_violation);
    }

    #[test]
    fn hypothesis_normalized_score_examples() {
        let perf = hypothesis_from_text("高速なAPI");
        let memory = hypothesis_from_text("メモリ512MB以下");
        let mixed = hypothesis_from_text("メモリ512MB以下。高速なAPI");

        assert!(perf.normalized_score > 0.0);
        assert!(memory.normalized_score < 0.0);
        assert!(mixed.normalized_score > 0.0);
    }
}
