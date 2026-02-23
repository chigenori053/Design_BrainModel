use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::BTreeMap;

use core_types::ObjectiveVector;
use design_reasoning::{
    HypothesisEngine, LanguageEngine, MeaningEngine, ProjectionEngine, SnapshotEngine,
};
use dhm::Dhm;
use field_engine::{FieldEngine, TargetField};
use knowledge_store::KnowledgeStore;
use language_dhm::{LangId, LanguageDhm, LanguageUnit};
use memory_space::{DesignState, MemoryInterferenceTelemetry};
use memory_store::{FileStore, InMemoryStore};
use recomposer::{
    DecisionReport, DesignReport, Recomposer, ResonanceReport,
};
use semantic_dhm::{ConceptUnit, SemanticDhm, SemanticL1Dhm, SemanticUnitL1};

mod ops;

use serde::{Deserialize, Serialize};

pub use chm::Chm;
pub use design_reasoning::{DesignHypothesis, Explanation, MeaningLayerSnapshotV2, SnapshotDiffV2};
pub use knowledge_store::{FeedbackAction, FeedbackEntry};
pub use recomposer::{ActionType, DecisionWeights, Recommendation};
pub use semantic_dhm::{
    ConceptId, DerivedRequirement, DesignProjection, L1Id, L2Config, L2Mode, MeaningLayerSnapshot,
    RequirementKind, RequirementRole as L1RequirementRole, SemanticError, SemanticUnitL1Input,
    Snapshotable, ConceptUnitV2, SemanticUnitL1V2, SemanticUnitL1Framework, SemanticUnitL2Detail,
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

#[derive(Clone, Debug, PartialEq)]
pub struct ConceptImpact {
    pub concept_id: ConceptId,
    pub original_stability: f64,
    pub simulated_stability: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SimulationReport {
    pub original_objectives: ObjectiveVector,
    pub simulated_objectives: ObjectiveVector,
    pub affected_concepts: Vec<ConceptImpact>,
    pub total_concepts: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BlastRadiusScore {
    pub coverage: f64,
    pub intensity: f64,
    pub structural_risk: f64,
    pub total_score: f64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InfoCategory {
    Constraint,
    Boundary,
    Metric,
    Objective,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MissingInfo {
    pub target_id: Option<L1Id>,
    pub category: InfoCategory,
    pub prompt: String,
    pub importance: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DesignDraft {
    pub draft_id: String,
    pub added_units: Vec<SemanticUnitL1V2>,
    pub stability_impact: f64,
    pub prompt: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArtifactFormat {
    Rust,
    Sql,
    Mermaid,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedArtifact {
    pub file_name: String,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq)]
struct ParetoPoint {
    idx: usize,
    stability_gain: f64,
    ambiguity_cost: f64,
    complexity_cost: f64,
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
    knowledge_store: KnowledgeStore,
    l2_grounding: BTreeMap<ConceptId, Vec<String>>,
    l2_refinements: BTreeMap<ConceptId, Vec<String>>,
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
            language_engine: LanguageEngine::new(),
            snapshot_engine: SnapshotEngine,
            recomposer: Recomposer,
            knowledge_store: {
                let mut ks = KnowledgeStore::new();
                ks.preload_defaults();
                ks
            },
            l2_grounding: BTreeMap::new(),
            l2_refinements: BTreeMap::new(),
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

    pub fn analyze_incremental(&mut self, text: &str) -> Result<ConceptUnit, SemanticError> {
        self.analyze_text(text)
    }

    pub fn add_knowledge(&mut self, topic: &str, vector: Vec<f32>) {
        self.knowledge_store.add_knowledge(topic, vector);
    }

    pub fn record_feedback(&mut self, draft_id: &str, action: FeedbackAction) {
        self.knowledge_store.record_feedback(draft_id, action);
    }

    pub fn adjust_weights(&mut self) {
        self.knowledge_store.adjust_weights();
    }

    pub fn feedback_entries(&self) -> Vec<FeedbackEntry> {
        self.knowledge_store.feedback_entries().to_vec()
    }

    pub fn load_feedback_entries(&mut self, entries: Vec<FeedbackEntry>) {
        self.knowledge_store.load_feedback_entries(entries);
    }

    pub fn clear_context(&mut self) -> Result<(), SemanticError> {
        let ids = self
            .semantic_l1_dhm
            .all_units()
            .into_iter()
            .map(|u| u.id)
            .collect::<Vec<_>>();
        for id in ids {
            self.semantic_l1_dhm
                .remove(id)
                .map_err(|e| SemanticError::EvaluationError(e.to_string()))?;
        }
        self.semantic_dhm
            .rebuild_l2_from_l1(&[])
            .map_err(|e| SemanticError::EvaluationError(format!("{e:?}")))?;
        Ok(())
    }

    pub fn generate_drafts(&self) -> Result<Vec<DesignDraft>, SemanticError> {
        let l1_units = self.all_l1_units_v2()?;
        let l2_units = self.project_phase_a_v2()?;
        let missing = self.extract_missing_information()?;
        let objective_text = l1_units
            .iter()
            .filter_map(|u| u.objective.clone())
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        let stability = if l2_units.is_empty() {
            0.0
        } else {
            l2_units.iter().map(|u| u.stability_score).sum::<f64>() / l2_units.len() as f64
        };
        let dashboard_trigger = objective_text.contains("ダッシュボード") || objective_text.contains("dashboard");
        let trigger = stability < 0.75 || !missing.is_empty() || dashboard_trigger;
        if !trigger {
            return Ok(Vec::new());
        }

        let mut related = self
            .knowledge_store
            .top_related_labels(&[0.7, 0.3, 0.2, 0.1], 3);
        if dashboard_trigger && !related.iter().any(|s| s == "権限管理") {
            related.insert(0, "権限管理".to_string());
        }
        let mut drafts = Vec::new();
        for (idx, topic) in related.into_iter().enumerate() {
            let added = SemanticUnitL1V2 {
                id: L1Id(0),
                objective: Some(format!("{topic} 機能を追加する")),
                scope_in: vec!["system".to_string()],
                scope_out: vec![],
                constraints: vec!["既存API互換を維持する".to_string()],
                ambiguity_score: 0.2,
            };
            let impact = 0.08 - (idx as f64 * 0.01);
            drafts.push(DesignDraft {
                draft_id: format!("DRAFT-{:03}-{}", idx + 1, topic),
                added_units: vec![added],
                stability_impact: impact,
                prompt: format!(
                    "{} を仮定すると、安定性が約{:.0}%向上する見込みです。採用しますか？",
                    topic,
                    (impact * 100.0).max(0.0)
                ),
            });
        }
        Ok(drafts)
    }

    pub fn commit_draft(&mut self, draft_id: &str) -> Result<(), SemanticError> {
        let drafts = self.generate_drafts()?;
        let draft = drafts
            .into_iter()
            .find(|d| d.draft_id == draft_id)
            .ok_or_else(|| SemanticError::InvalidInput("draft not found".to_string()))?;
        for unit in draft.added_units {
            let objective = unit.objective.unwrap_or_else(|| "generated".to_string());
            let input = SemanticUnitL1Input {
                role: L1RequirementRole::Constraint,
                polarity: 1,
                abstraction: 0.35,
                vector: vector_from_text(&objective),
                source_text: objective,
            };
            let _ = self.semantic_l1_dhm.insert(&input);
        }
        self.rebuild_l2_from_l1_v2()?;
        Ok(())
    }

    pub fn pareto_optimize_drafts(&self, drafts: Vec<DesignDraft>) -> Vec<DesignDraft> {
        if drafts.len() <= 1 {
            return drafts;
        }

        let points = drafts
            .iter()
            .enumerate()
            .map(|(idx, d)| {
                let ambiguity = if d.added_units.is_empty() {
                    0.0
                } else {
                    d.added_units.iter().map(|u| u.ambiguity_score).sum::<f64>() / d.added_units.len() as f64
                };
                let complexity = d
                    .added_units
                    .iter()
                    .map(|u| (u.constraints.len() + u.scope_in.len() + u.scope_out.len()) as f64)
                    .sum::<f64>();
                ParetoPoint {
                    idx,
                    stability_gain: d.stability_impact,
                    ambiguity_cost: ambiguity,
                    complexity_cost: complexity,
                }
            })
            .collect::<Vec<_>>();

        let mut rank = vec![0usize; drafts.len()];
        for i in 0..points.len() {
            rank[i] = points
                .iter()
                .enumerate()
                .filter(|(j, p)| *j != i && dominates(p, &points[i]))
                .count();
        }

        let mut indexed = drafts.into_iter().enumerate().collect::<Vec<_>>();
        indexed.sort_by(|(li, l), (ri, r)| {
            rank[*li]
                .cmp(&rank[*ri])
                .then_with(|| r.stability_impact.total_cmp(&l.stability_impact))
                .then_with(|| l.draft_id.cmp(&r.draft_id))
        });
        indexed.into_iter().map(|(_, d)| d).collect()
    }

    pub fn generate_artifacts(
        &self,
        format: ArtifactFormat,
    ) -> Result<Vec<GeneratedArtifact>, SemanticError> {
        let l2_units = self.project_phase_a_v2()?;
        let artifacts = match format {
            ArtifactFormat::Rust => generate_rust_artifacts(&l2_units),
            ArtifactFormat::Sql => generate_sql_artifacts(&l2_units),
            ArtifactFormat::Mermaid => generate_mermaid_artifacts(&l2_units),
        };
        Ok(artifacts)
    }

    #[deprecated(since = "PhaseA-Final", note = "Will be removed in PhaseC. Use get_l1_unit_v2")]
    pub fn get_l1_unit(&self, id: L1Id) -> Option<SemanticUnitL1> {
        self.semantic_l1_dhm.get(id)
    }

    pub fn get_l1_unit_v2(&self, id: L1Id) -> Result<Option<SemanticUnitL1V2>, SemanticError> {
        self.semantic_l1_dhm
            .get(id)
            .map(|u| SemanticUnitL1V2::try_from(u))
            .transpose()
    }

    #[deprecated(since = "PhaseA-Final", note = "Will be removed in PhaseC. Use all_l1_units_v2")]
    pub fn all_l1_units(&self) -> Vec<SemanticUnitL1> {
        self.semantic_l1_dhm.all_units()
    }

    pub fn all_l1_units_v2(&self) -> Result<Vec<SemanticUnitL1V2>, SemanticError> {
        self.semantic_l1_dhm
            .all_units()
            .into_iter()
            .map(SemanticUnitL1V2::try_from)
            .collect()
    }

    pub fn remove_l1(&mut self, id: L1Id) -> Result<(), HybridVmError> {
        self.semantic_l1_dhm.remove(id).map_err(HybridVmError::Io)
    }

    #[deprecated(since = "PhaseA-Final", note = "Will be removed in PhaseC. Use rebuild_l2_from_l1_v2")]
    pub fn rebuild_l2_from_l1(&mut self) -> Result<(), SemanticError> {
        ops::semantic::rebuild_l2_from_l1(&self.semantic_l1_dhm, &mut self.semantic_dhm)
    }

    pub fn rebuild_l2_from_l1_v2(&mut self) -> Result<Vec<ConceptUnitV2>, SemanticError> {
        ops::semantic::rebuild_l2_from_l1(&self.semantic_l1_dhm, &mut self.semantic_dhm)?;
        self.semantic_dhm
            .all_concepts()
            .into_iter()
            .map(ConceptUnitV2::try_from)
            .collect()
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

    #[deprecated(since = "PhaseA-Final", note = "Will be removed in PhaseC. Use snapshot_v2")]
    pub fn snapshot(&self) -> Result<MeaningLayerSnapshot, SemanticError> {
        ops::semantic::snapshot(&self.snapshot_engine, &self.semantic_l1_dhm, &self.semantic_dhm)
    }

    pub fn snapshot_v2(&self) -> Result<MeaningLayerSnapshotV2, SemanticError> {
        ops::semantic::snapshot_v2(&self.snapshot_engine, &self.semantic_l1_dhm, &self.semantic_dhm)
    }

    #[deprecated(since = "PhaseA-Final", note = "Will be removed in PhaseC. Use compare_snapshots_v2")]
    pub fn compare_snapshots(
        &self,
        left: &MeaningLayerSnapshot,
        right: &MeaningLayerSnapshot,
    ) -> Result<semantic_dhm::SnapshotDiff, SemanticError> {
        self.snapshot_engine.compare(left, right)
    }

    pub fn compare_snapshots_v2(
        &self,
        left: &MeaningLayerSnapshotV2,
        right: &MeaningLayerSnapshotV2,
    ) -> SnapshotDiffV2 {
        ops::semantic::compare_snapshots_v2(&self.snapshot_engine, left, right)
    }

    #[deprecated(since = "PhaseA-Final", note = "Will be removed in PhaseC. Use project_phase_a_v2")]
    pub fn project_phase_a(&self) -> DesignProjection {
        ops::semantic::project_phase_a(&self.projection_engine, &self.semantic_l1_dhm, &self.semantic_dhm)
    }

    pub fn project_phase_a_v2(&self) -> Result<Vec<ConceptUnitV2>, SemanticError> {
        self.semantic_dhm
            .all_concepts()
            .into_iter()
            .map(ConceptUnitV2::try_from)
            .collect()
    }

    pub fn simulate_perturbation(
        &self,
        target_l1: L1Id,
        delta_abstraction: f32,
    ) -> Result<SimulationReport, SemanticError> {
        let l1_units = self.all_l1_units_v2()?;
        let l2_units = self.project_phase_a_v2()?;
        let concepts = self.semantic_dhm.all_concepts();
        let original = objective_from_units(&l1_units, &l2_units).clamped();

        let mut impacts = Vec::new();
        let mut simulated_stability = l2_units
            .iter()
            .map(|c| (c.id, c.stability_score))
            .collect::<std::collections::BTreeMap<_, _>>();

        for c in concepts {
            if c.l1_refs.contains(&target_l1) {
                let Some(base) = simulated_stability.get(&c.id).copied() else {
                    continue;
                };
                let moved = (base + (delta_abstraction as f64) * 0.20).clamp(0.0, 1.0);
                simulated_stability.insert(c.id, moved);
                impacts.push(ConceptImpact {
                    concept_id: c.id,
                    original_stability: base,
                    simulated_stability: moved,
                });
            }
        }

        let mean_stability = if simulated_stability.is_empty() {
            0.0
        } else {
            simulated_stability.values().sum::<f64>() / simulated_stability.len() as f64
        };
        let mut simulated = original.clone();
        simulated.f_struct = mean_stability.clamp(0.0, 1.0);
        simulated.f_risk = (original.f_risk + impacts.len() as f64 / (simulated_stability.len().max(1) as f64) * 0.1)
            .clamp(0.0, 1.0);
        simulated.f_shape = (1.0 - mean_stability * 0.5).clamp(0.0, 1.0);

        Ok(SimulationReport {
            original_objectives: original,
            simulated_objectives: simulated.clamped(),
            affected_concepts: impacts,
            total_concepts: simulated_stability.len(),
        })
    }

    pub fn simulate_removal(&self, target_l1: L1Id) -> Result<SimulationReport, SemanticError> {
        self.simulate_perturbation(target_l1, -1.0)
    }

    pub fn evaluate_blast_radius(&self, report: &SimulationReport) -> BlastRadiusScore {
        let total = report.total_concepts.max(1) as f64;
        let coverage = (report.affected_concepts.len() as f64 / total).clamp(0.0, 1.0);
        let intensity = if report.affected_concepts.is_empty() {
            0.0
        } else {
            report
                .affected_concepts
                .iter()
                .map(|c| (c.simulated_stability - c.original_stability).abs())
                .sum::<f64>()
                / report.affected_concepts.len() as f64
        }
        .clamp(0.0, 1.0);

        let l2_units = self.project_phase_a_v2().unwrap_or_default();
        let avg_links = if l2_units.is_empty() {
            0.0
        } else {
            l2_units.iter().map(|u| u.causal_links.len() as f64).sum::<f64>() / l2_units.len() as f64
        };
        let mut structural_risk = 0.0;
        for impact in &report.affected_concepts {
            if let Some(unit) = l2_units.iter().find(|u| u.id == impact.concept_id) {
                let is_hub = avg_links > 0.0 && (unit.causal_links.len() as f64) >= avg_links * 2.0;
                let dropped = (impact.original_stability - impact.simulated_stability).max(0.0);
                if is_hub && dropped > 0.0 {
                    structural_risk += dropped;
                }
            }
        }
        structural_risk = structural_risk.clamp(0.0, 1.0);

        let total_score = (coverage * 0.4 + intensity * 0.35 + structural_risk * 0.25).clamp(0.0, 1.0);
        BlastRadiusScore {
            coverage,
            intensity,
            structural_risk,
            total_score,
        }
    }

    pub fn extract_missing_information(&self) -> Result<Vec<MissingInfo>, SemanticError> {
        let l1_units = self.all_l1_units_v2()?;
        let l2_units = self.project_phase_a_v2()?;
        let mut out = Vec::new();

        for l1 in &l1_units {
            if l1.constraints.is_empty() && l1.ambiguity_score > 0.7 {
                out.push(MissingInfo {
                    target_id: Some(l1.id),
                    category: InfoCategory::Constraint,
                    prompt: "制約は何ですか？（性能上限、コスト、期限、法規制など）".to_string(),
                    importance: 0.95,
                });
            }
            let objective_text = l1.objective.clone().unwrap_or_default().to_lowercase();
            if objective_text.contains("開発したい")
                || objective_text.contains("構築")
                || objective_text.contains("develop")
                || objective_text.contains("build")
            {
                let related = self.knowledge_store.top_related_labels(&[0.6, 0.4, 0.2, 0.1], 2);
                let hint = if related.is_empty() {
                    "対象範囲と非対象範囲（Boundary）を明示してください。".to_string()
                } else {
                    format!(
                        "対象範囲と非対象範囲（Boundary）を明示してください。関連定石: {}",
                        related.join(" / ")
                    )
                };
                out.push(MissingInfo {
                    target_id: Some(l1.id),
                    category: InfoCategory::Boundary,
                    prompt: hint,
                    importance: 0.80,
                });
            }
        }

        for l2 in &l2_units {
            let has_pos = l2.derived_requirements.iter().any(|r| r.strength > 0.0);
            let has_neg = l2.derived_requirements.iter().any(|r| r.strength < 0.0);
            if has_pos && has_neg {
                out.push(MissingInfo {
                    target_id: None,
                    category: InfoCategory::Objective,
                    prompt: format!(
                        "L2-{} で要件競合が検出されました。優先順位（何を先に最適化するか）を決めてください。",
                        l2.id.0
                    ),
                    importance: 0.85,
                });
            }
        }

        out.sort_by(|a, b| b.importance.total_cmp(&a.importance));
        Ok(out)
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

    pub fn explain_design_v2(&mut self, text: &str) -> Result<Explanation, SemanticError> {
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

    #[deprecated(since = "PhaseA-Final", note = "Will be removed in PhaseC. Use explain_design_v2")]
    pub fn explain_design(&mut self, text: &str) -> Result<Explanation, SemanticError> {
        self.explain_design_v2(text)
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
            language_engine: LanguageEngine::new(),
            snapshot_engine: SnapshotEngine,
            recomposer: Recomposer,
            knowledge_store: {
                let mut ks = KnowledgeStore::new();
                ks.preload_defaults();
                ks
            },
            l2_grounding: BTreeMap::new(),
            l2_refinements: BTreeMap::new(),
            mode: ExecutionMode::RecallFirst,
            trace: Vec::new(),
        })
    }

    pub fn create_l1_framework(&mut self, input: &str) -> Result<SemanticUnitL1Framework, SemanticError> {
        let normalized = input.trim();
        if normalized.is_empty() {
            return Err(SemanticError::InvalidInput("empty input".to_string()));
        }
        let insert = SemanticUnitL1Input {
            role: L1RequirementRole::Goal,
            polarity: 1,
            abstraction: 0.7,
            vector: vector_from_text(normalized),
            source_text: normalized.to_string(),
        };
        let id = self.semantic_l1_dhm.insert(&insert);
        let l1 = self
            .semantic_l1_dhm
            .get(id)
            .ok_or_else(|| SemanticError::EvaluationError("failed to read inserted L1".to_string()))?;
        let l1_v2 = SemanticUnitL1V2::try_from(l1)?;
        Ok(SemanticUnitL1Framework::from_l1_v2(&l1_v2))
    }

    pub fn derive_l2_detail(&mut self, l1_id: L1Id) -> Result<SemanticUnitL2Detail, SemanticError> {
        self.rebuild_l2_from_l1_v2()?;
        let concept = self
            .semantic_dhm
            .all_concepts()
            .into_iter()
            .find(|c| c.l1_refs.contains(&l1_id))
            .ok_or_else(|| SemanticError::MissingField("l2_detail_for_l1"))?;
        let concept_v2 = ConceptUnitV2::try_from(concept.clone())?;
        let mut detail = SemanticUnitL2Detail::from_concept_v2(l1_id, &concept_v2);
        if let Some(grounding) = self.l2_grounding.get(&concept.id) {
            detail.grounding_data = grounding.clone();
        }
        Ok(detail)
    }

    pub fn update_l2_with_grounding(&mut self, l2_id: ConceptId, knowledge: &str) -> Result<(), SemanticError> {
        if knowledge.trim().is_empty() {
            return Err(SemanticError::InvalidInput("grounding knowledge is empty".to_string()));
        }
        let exists = self.semantic_dhm.get(l2_id).is_some();
        if !exists {
            return Err(SemanticError::MissingField("l2_id"));
        }
        self.l2_grounding
            .entry(l2_id)
            .or_default()
            .push(knowledge.trim().to_string());
        Ok(())
    }

    pub fn list_l2_details(&mut self) -> Result<Vec<SemanticUnitL2Detail>, SemanticError> {
        self.rebuild_l2_from_l1_v2()?;
        let details = self
            .semantic_dhm
            .all_concepts()
            .into_iter()
            .filter_map(|concept| {
                let parent_id = concept.l1_refs.first().copied()?;
                let concept_id = concept.id;
                let concept_v2 = ConceptUnitV2::try_from(concept).ok()?;
                let mut detail = SemanticUnitL2Detail::from_concept_v2(parent_id, &concept_v2);
                if let Some(g) = self.l2_grounding.get(&concept_id) {
                    detail.grounding_data.extend(g.clone());
                }
                if let Some(r) = self.l2_refinements.get(&concept_id) {
                    detail.methods.extend(r.clone());
                }
                Some(detail)
            })
            .collect::<Vec<_>>();
        Ok(details)
    }

    pub fn card_has_knowledge_gap(&mut self, l2_id: ConceptId) -> Result<bool, SemanticError> {
        let detail = self
            .list_l2_details()?
            .into_iter()
            .find(|d| d.id == l2_id)
            .ok_or(SemanticError::MissingField("card_id"))?;
        Ok(detail.grounding_data.is_empty() && (!detail.metrics.is_empty() || !detail.methods.is_empty()))
    }

    pub fn run_grounding_search(
        &mut self,
        l2_id: ConceptId,
        query: &str,
    ) -> Result<Vec<String>, SemanticError> {
        if query.trim().is_empty() {
            return Err(SemanticError::InvalidInput("query is empty".to_string()));
        }
        let related = self
            .knowledge_store
            .top_related_labels(&vector_from_text(query), 3);
        let mut out = Vec::new();
        for label in related {
            let line = format!("Grounded reference: {label} (query={})", query.trim());
            self.update_l2_with_grounding(l2_id, &line)?;
            out.push(line);
        }
        Ok(out)
    }

    pub fn refine_l2_detail(&mut self, l2_id: ConceptId, detail_text: &str) -> Result<(), SemanticError> {
        let text = detail_text.trim();
        if text.is_empty() {
            return Err(SemanticError::InvalidInput("detail text is empty".to_string()));
        }
        let concept = self
            .semantic_dhm
            .get(l2_id)
            .ok_or(SemanticError::MissingField("card_id"))?;
        let parent = concept
            .l1_refs
            .first()
            .copied()
            .ok_or(SemanticError::MissingField("parent_l1"))?;
        let input = SemanticUnitL1Input {
            role: L1RequirementRole::Constraint,
            polarity: -1,
            abstraction: 0.35,
            vector: vector_from_text(text),
            source_text: format!("L2-{} refinement: {}", l2_id.0, text),
        };
        let _ = parent;
        let _ = self.semantic_l1_dhm.insert(&input);
        self.l2_refinements
            .entry(l2_id)
            .or_default()
            .push(text.to_string());
        self.rebuild_l2_from_l1_v2()?;
        Ok(())
    }

    pub fn export_l2_grounding(&self) -> Vec<(u64, Vec<String>)> {
        self.l2_grounding
            .iter()
            .map(|(k, v)| (k.0, v.clone()))
            .collect()
    }

    pub fn load_l2_grounding(&mut self, data: Vec<(u64, Vec<String>)>) {
        self.l2_grounding = data
            .into_iter()
            .map(|(k, v)| (ConceptId(k), v))
            .collect::<BTreeMap<_, _>>();
    }

    pub fn export_l2_refinements(&self) -> Vec<(u64, Vec<String>)> {
        self.l2_refinements
            .iter()
            .map(|(k, v)| (k.0, v.clone()))
            .collect()
    }

    pub fn load_l2_refinements(&mut self, data: Vec<(u64, Vec<String>)>) {
        self.l2_refinements = data
            .into_iter()
            .map(|(k, v)| (ConceptId(k), v))
            .collect::<BTreeMap<_, _>>();
    }

    /// RFC-013: 内部構造をユーザー向けの「デザインカード」形式へ変換する
    pub fn get_design_cards(&mut self) -> Result<Vec<DesignCard>, SemanticError> {
        let l1_units = self.all_l1_units_v2()?;
        let mut cards = Vec::new();

        for l1 in l1_units {
            let framework = semantic_dhm::SemanticUnitL1Framework::from_l1_v2(&l1);
            let detail = self.derive_l2_detail(l1.id).ok(); // 詳細がない場合は None

            let mut card = DesignCard {
                id: format!("CARD-{}", l1.id.0),
                title: framework.title.clone(),
                overview: framework.objective.clone(),
                details: Vec::new(),
                status: CardStatus::Hypothetical,
            };

            if let Some(d) = detail {
                for m in d.methods {
                    card.details.push(format!("Method: {}", m));
                }
                for metric in d.metrics {
                    card.details.push(format!("Metric: {}", metric));
                }
                if !d.grounding_data.is_empty() {
                    card.status = CardStatus::Grounded;
                    for g in d.grounding_data {
                        card.details.push(format!("Grounding: {}", g));
                    }
                }
            }

            cards.push(card);
        }

        Ok(cards)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum CardStatus {
    Hypothetical,
    Grounded,
    Confirmed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DesignCard {
    pub id: String,
    pub title: String,
    pub overview: String,
    pub details: Vec<String>,
    pub status: CardStatus,
}

fn vector_from_text(text: &str) -> Vec<f32> {
    let mut out = vec![0.0f32; 8];
    let n = out.len();
    for (i, b) in text.bytes().enumerate() {
        out[i % n] += (b as f32) / 255.0;
    }
    let norm = out.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 1e-6 {
        for v in &mut out {
            *v /= norm;
        }
    }
    out
}

fn generate_rust_artifacts(l2_units: &[ConceptUnitV2]) -> Vec<GeneratedArtifact> {
    l2_units
        .iter()
        .map(|concept| {
            let mut content = String::new();
            content.push_str("// Auto-generated by RFC-012 Artifact Transformer\n");
            content.push_str(&format!(
                "// source_concept: L2-{}, trace_hash: {:016x}\n\n",
                concept.id.0,
                trace_hash_for_concept(concept)
            ));
            content.push_str("#[derive(Debug, Clone)]\n");
            content.push_str(&format!(
                "pub struct Concept{}Service {{\n    pub concept_id: u64,\n}}\n\n",
                concept.id.0
            ));
            content.push_str(&format!(
                "pub trait Concept{}Behavior {{\n    fn execute(&self) -> Result<(), String>;\n}}\n\n",
                concept.id.0
            ));
            content.push_str(&format!(
                "impl Concept{}Behavior for Concept{}Service {{\n",
                concept.id.0, concept.id.0
            ));
            content.push_str("    fn execute(&self) -> Result<(), String> {\n");
            for req in &concept.derived_requirements {
                content.push_str(&format!(
                    "        // requirement: {:?} (strength={:.2})\n",
                    req.kind, req.strength
                ));
            }
            for link in &concept.causal_links {
                content.push_str(&format!(
                    "        // dependency: L1-{} -> L1-{} (weight={:.3})\n",
                    link.from.0, link.to.0, link.weight
                ));
            }
            content.push_str("        Ok(())\n    }\n}\n");

            GeneratedArtifact {
                file_name: format!("concept_{}.rs", concept.id.0),
                content,
            }
        })
        .collect()
}

fn generate_sql_artifacts(l2_units: &[ConceptUnitV2]) -> Vec<GeneratedArtifact> {
    let mut content = String::new();
    content.push_str("-- Auto-generated by RFC-012 Artifact Transformer\n\n");
    content.push_str("CREATE TABLE IF NOT EXISTS l2_concepts (\n");
    content.push_str("  id BIGINT PRIMARY KEY,\n");
    content.push_str("  stability_score DOUBLE PRECISION NOT NULL,\n");
    content.push_str("  trace_hash VARCHAR(32) NOT NULL\n");
    content.push_str(");\n\n");
    content.push_str("CREATE TABLE IF NOT EXISTS l2_derived_requirements (\n");
    content.push_str("  concept_id BIGINT NOT NULL,\n");
    content.push_str("  kind VARCHAR(32) NOT NULL,\n");
    content.push_str("  strength DOUBLE PRECISION NOT NULL,\n");
    content.push_str("  FOREIGN KEY (concept_id) REFERENCES l2_concepts(id)\n");
    content.push_str(");\n\n");
    content.push_str("CREATE TABLE IF NOT EXISTS l2_causal_links (\n");
    content.push_str("  concept_id BIGINT NOT NULL,\n");
    content.push_str("  from_l1 VARCHAR(64) NOT NULL,\n");
    content.push_str("  to_l1 VARCHAR(64) NOT NULL,\n");
    content.push_str("  weight DOUBLE PRECISION NOT NULL,\n");
    content.push_str("  FOREIGN KEY (concept_id) REFERENCES l2_concepts(id)\n");
    content.push_str(");\n\n");
    for concept in l2_units {
        content.push_str(&format!(
            "INSERT INTO l2_concepts (id, stability_score, trace_hash) VALUES ({}, {:.6}, '{:016x}');\n",
            concept.id.0,
            concept.stability_score,
            trace_hash_for_concept(concept)
        ));
        for req in &concept.derived_requirements {
            content.push_str(&format!(
                "INSERT INTO l2_derived_requirements (concept_id, kind, strength) VALUES ({}, '{:?}', {:.6});\n",
                concept.id.0, req.kind, req.strength
            ));
        }
        for link in &concept.causal_links {
            content.push_str(&format!(
                "INSERT INTO l2_causal_links (concept_id, from_l1, to_l1, weight) VALUES ({}, '{}', '{}', {:.6});\n",
                concept.id.0, link.from.0, link.to.0, link.weight
            ));
        }
    }
    vec![GeneratedArtifact {
        file_name: "schema.sql".to_string(),
        content,
    }]
}

fn generate_mermaid_artifacts(l2_units: &[ConceptUnitV2]) -> Vec<GeneratedArtifact> {
    let mut content = String::new();
    content.push_str("%% Auto-generated by RFC-012 Artifact Transformer\n");
    content.push_str("graph TD\n");
    for concept in l2_units {
        content.push_str(&format!(
            "  L2_{}[\"L2-{} stability={:.2}\"]\n",
            concept.id.0, concept.id.0, concept.stability_score
        ));
    }
    for concept in l2_units {
        for link in &concept.causal_links {
            content.push_str(&format!(
                "  L2_{} -->|L1 {}->{} ({:.2})| L2_{}\n",
                concept.id.0, link.from.0, link.to.0, link.weight, concept.id.0
            ));
        }
    }
    vec![GeneratedArtifact {
        file_name: "graph.mmd".to_string(),
        content,
    }]
}

fn trace_hash_for_concept(concept: &ConceptUnitV2) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    concept.id.0.hash(&mut hasher);
    concept.stability_score.to_bits().hash(&mut hasher);
    concept.derived_requirements.len().hash(&mut hasher);
    concept.causal_links.len().hash(&mut hasher);
    hasher.finish()
}

fn dominates(a: &ParetoPoint, b: &ParetoPoint) -> bool {
    let not_worse = a.stability_gain >= b.stability_gain
        && a.ambiguity_cost <= b.ambiguity_cost
        && a.complexity_cost <= b.complexity_cost;
    let strictly_better = a.stability_gain > b.stability_gain
        || a.ambiguity_cost < b.ambiguity_cost
        || a.complexity_cost < b.complexity_cost;
    not_worse && strictly_better
}

fn objective_from_units(l1_units: &[SemanticUnitL1V2], l2_units: &[ConceptUnitV2]) -> ObjectiveVector {
    let f_struct = if l2_units.is_empty() {
        0.0
    } else {
        l2_units.iter().map(|u| u.stability_score).sum::<f64>() / l2_units.len() as f64
    };
    let mean_ambiguity = if l1_units.is_empty() {
        1.0
    } else {
        l1_units.iter().map(|u| u.ambiguity_score).sum::<f64>() / l1_units.len() as f64
    };
    let avg_links = if l2_units.is_empty() {
        0.0
    } else {
        l2_units.iter().map(|u| u.causal_links.len() as f64).sum::<f64>() / l2_units.len() as f64
    };
    let max_links = l2_units.iter().map(|u| u.causal_links.len()).max().unwrap_or(1) as f64;
    ObjectiveVector {
        f_struct: f_struct.clamp(0.0, 1.0),
        f_field: (1.0 - mean_ambiguity).clamp(0.0, 1.0),
        f_risk: (avg_links / (max_links + 1.0)).clamp(0.0, 1.0),
        f_shape: (1.0 - f_struct * 0.5).clamp(0.0, 1.0),
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

    use crate::{
        Evaluator, ExecutionContext, ExecutionMode, Explanation, HybridVM, MeaningLayerSnapshotV2,
        StructuralEvaluator,
    };

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

    #[test]
    fn snapshot_v2_compare_ignores_timestamp() {
        let store_dir = std::env::temp_dir().join(format!(
            "hybrid_vm_snapshot_v2_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let vm = HybridVM::for_cli_storage(&store_dir).expect("vm");
        let mut a = vm.snapshot_v2().expect("snapshot_v2 a");
        let b = vm.snapshot_v2().expect("snapshot_v2 b");
        a.timestamp_ms = a.timestamp_ms.saturating_add(1_000);
        let diff = vm.compare_snapshots_v2(&a, &b);
        assert!(!diff.l1_changed);
        assert!(!diff.l2_changed);
    }

    #[test]
    fn l1_and_l2_v2_api_available() {
        let mut vm = HybridVM::with_default_memory(StructuralEvaluator::default()).expect("vm");
        let concept = vm
            .analyze_text("高性能かつクラウド依存禁止")
            .expect("analyze");
        let first_l1 = concept.l1_refs[0];
        let l1_v2 = vm.get_l1_unit_v2(first_l1).expect("get l1 v2");
        assert!(l1_v2.is_some());
        let projected_v2 = vm.project_phase_a_v2().expect("project v2");
        assert!(!projected_v2.is_empty());
        let rebuilt_v2 = vm.rebuild_l2_from_l1_v2().expect("rebuild v2");
        assert!(!rebuilt_v2.is_empty());
    }

    #[test]
    fn deterministic_outputs_across_100_runs() {
        let input = "高速なAPI。クラウド依存は禁止。メモリ512MB以下";
        let mut first_snapshot: Option<MeaningLayerSnapshotV2> = None;
        let mut first_explain: Option<Explanation> = None;
        for n in 0..100 {
            let store_dir = std::env::temp_dir().join(format!(
                "hybrid_vm_det_{}_{}",
                n,
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("clock")
                    .as_nanos()
            ));
            let mut vm = HybridVM::for_cli_storage(&store_dir).expect("vm");
            let _ = vm.analyze_text(input).expect("analyze");
            let snapshot = vm.snapshot_v2().expect("snapshot_v2");
            let explain = vm.explain_design_v2(input).expect("explain v2");
            if let Some(expected) = &first_snapshot {
                assert_eq!(expected.l1_hash, snapshot.l1_hash);
                assert_eq!(expected.l2_hash, snapshot.l2_hash);
                assert_eq!(expected.version, snapshot.version);
            } else {
                first_snapshot = Some(snapshot);
            }
            if let Some(expected) = &first_explain {
                assert_eq!(expected, &explain);
            } else {
                first_explain = Some(explain);
            }
        }
    }

    #[test]
    fn rfc014_framework_and_detail_flow() {
        let store_dir = std::env::temp_dir().join(format!(
            "hybrid_vm_rfc014_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut vm = HybridVM::for_cli_storage(&store_dir).expect("vm");
        let framework = vm
            .create_l1_framework("決済APIの信頼性を向上させる")
            .expect("framework");
        assert!(!framework.title.is_empty());
        assert!(!framework.objective.is_empty());

        let detail = vm.derive_l2_detail(framework.id).expect("detail");
        assert_eq!(detail.parent_id, framework.id);
        assert!(!detail.metrics.is_empty());
        assert!(!detail.methods.is_empty());
    }

    #[test]
    fn rfc014_grounding_update_is_reflected_in_detail() {
        let store_dir = std::env::temp_dir().join(format!(
            "hybrid_vm_rfc014_grounding_{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let mut vm = HybridVM::for_cli_storage(&store_dir).expect("vm");
        let framework = vm
            .create_l1_framework("認可処理を強化する")
            .expect("framework");
        let detail = vm.derive_l2_detail(framework.id).expect("detail");
        vm.update_l2_with_grounding(detail.id, "OWASP ASVS controls")
            .expect("grounding update");
        let detail_after = vm.derive_l2_detail(framework.id).expect("detail after");
        assert!(detail_after
            .grounding_data
            .iter()
            .any(|g| g.contains("OWASP ASVS controls")));
    }
}
