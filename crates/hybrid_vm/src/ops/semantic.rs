use design_reasoning::{
    DesignHypothesis, Explanation, HypothesisEngine, LanguageEngine, MeaningEngine, ProjectionEngine,
    SnapshotEngine,
};
use language_dhm::{LangId, LanguageDhm, LanguageUnit};
use memory_store::FileStore;
use semantic_dhm::{
    ConceptId, ConceptUnit, L1Id, L2Config, L2Mode, MeaningLayerSnapshot, SemanticDhm, SemanticL1Dhm,
    SemanticUnitL1,
};

use crate::HybridVmError;

pub(crate) fn analyze_text(
    meaning_engine: &MeaningEngine,
    text: &str,
    language_dhm: &mut LanguageDhm<FileStore<LangId, LanguageUnit>>,
    semantic_l1_dhm: &mut SemanticL1Dhm<FileStore<L1Id, SemanticUnitL1>>,
    semantic_dhm: &mut SemanticDhm<FileStore<ConceptId, ConceptUnit>>,
) -> Result<ConceptUnit, HybridVmError> {
    meaning_engine
        .analyze_text(text, language_dhm, semantic_l1_dhm, semantic_dhm)
        .map_err(HybridVmError::Io)
}

pub(crate) fn rebuild_l2_from_l1(
    semantic_l1_dhm: &SemanticL1Dhm<FileStore<L1Id, SemanticUnitL1>>,
    semantic_dhm: &mut SemanticDhm<FileStore<ConceptId, ConceptUnit>>,
) -> Result<(), HybridVmError> {
    let l1 = semantic_l1_dhm.all_units();
    semantic_dhm.rebuild_l2_from_l1(&l1).map_err(HybridVmError::Io)
}

pub(crate) fn rebuild_l2_from_l1_with_config(
    semantic_l1_dhm: &SemanticL1Dhm<FileStore<L1Id, SemanticUnitL1>>,
    semantic_dhm: &mut SemanticDhm<FileStore<ConceptId, ConceptUnit>>,
    config: L2Config,
) -> Result<(), HybridVmError> {
    let l1 = semantic_l1_dhm.all_units();
    semantic_dhm
        .rebuild_l2_from_l1_with_config(&l1, config)
        .map_err(HybridVmError::Io)
}

pub(crate) fn rebuild_l2_from_l1_with_mode(
    semantic_l1_dhm: &SemanticL1Dhm<FileStore<L1Id, SemanticUnitL1>>,
    semantic_dhm: &mut SemanticDhm<FileStore<ConceptId, ConceptUnit>>,
    mode: L2Mode,
) -> Result<(), HybridVmError> {
    let l1 = semantic_l1_dhm.all_units();
    semantic_dhm
        .rebuild_l2_from_l1_with_mode(&l1, mode)
        .map_err(HybridVmError::Io)
}

pub(crate) fn snapshot(
    snapshot_engine: &SnapshotEngine,
    semantic_l1_dhm: &SemanticL1Dhm<FileStore<L1Id, SemanticUnitL1>>,
    semantic_dhm: &SemanticDhm<FileStore<ConceptId, ConceptUnit>>,
) -> MeaningLayerSnapshot {
    snapshot_engine.snapshot(
        semantic_dhm.l2_config().algorithm_version,
        semantic_l1_dhm.all_units(),
        semantic_dhm.all_concepts(),
    )
}

pub(crate) fn project_phase_a(
    projection_engine: &ProjectionEngine,
    semantic_l1_dhm: &SemanticL1Dhm<FileStore<L1Id, SemanticUnitL1>>,
    semantic_dhm: &SemanticDhm<FileStore<ConceptId, ConceptUnit>>,
) -> semantic_dhm::DesignProjection {
    projection_engine.project_phase_a(&semantic_dhm.all_concepts(), &semantic_l1_dhm.all_units())
}

pub(crate) fn evaluate_design(
    text: &str,
    meaning_engine: &MeaningEngine,
    projection_engine: &ProjectionEngine,
    hypothesis_engine: &HypothesisEngine,
    language_dhm: &mut LanguageDhm<FileStore<LangId, LanguageUnit>>,
    semantic_l1_dhm: &mut SemanticL1Dhm<FileStore<L1Id, SemanticUnitL1>>,
    semantic_dhm: &mut SemanticDhm<FileStore<ConceptId, ConceptUnit>>,
) -> Result<DesignHypothesis, HybridVmError> {
    let _ = analyze_text(
        meaning_engine,
        text,
        language_dhm,
        semantic_l1_dhm,
        semantic_dhm,
    )?;
    let projection = project_phase_a(projection_engine, semantic_l1_dhm, semantic_dhm);
    Ok(hypothesis_engine.evaluate_hypothesis(&projection))
}

pub(crate) fn explain_design(
    text: &str,
    meaning_engine: &MeaningEngine,
    projection_engine: &ProjectionEngine,
    hypothesis_engine: &HypothesisEngine,
    language_engine: &LanguageEngine,
    language_dhm: &mut LanguageDhm<FileStore<LangId, LanguageUnit>>,
    semantic_l1_dhm: &mut SemanticL1Dhm<FileStore<L1Id, SemanticUnitL1>>,
    semantic_dhm: &mut SemanticDhm<FileStore<ConceptId, ConceptUnit>>,
) -> Result<Explanation, HybridVmError> {
    let _ = analyze_text(
        meaning_engine,
        text,
        language_dhm,
        semantic_l1_dhm,
        semantic_dhm,
    )?;
    let projection = project_phase_a(projection_engine, semantic_l1_dhm, semantic_dhm);
    let hypothesis = hypothesis_engine.evaluate_hypothesis(&projection);
    let state = language_engine.build_state(&projection, &semantic_l1_dhm.all_units(), &hypothesis);
    Ok(language_engine.explain_state(&state))
}
