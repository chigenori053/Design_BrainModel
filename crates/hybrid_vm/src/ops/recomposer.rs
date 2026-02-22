use recomposer::{
    DecisionReport, DecisionWeights, DesignReport, MultiConceptInput, RecommendationInput, Recomposer,
    ResonanceReport,
};
use semantic_dhm::{ConceptId, ConceptQuery, ConceptUnit, ResonanceWeights, SemanticDhm};

use memory_store::FileStore;

use crate::ops::util::{dedup_ids, dot_norm};
use crate::HybridVmError;

pub(crate) fn compare(
    semantic_dhm: &SemanticDhm<FileStore<ConceptId, ConceptUnit>>,
    left: ConceptId,
    right: ConceptId,
) -> Result<ResonanceReport, HybridVmError> {
    let Some(c1) = semantic_dhm.get(left) else {
        return Err(HybridVmError::ConceptNotFound(left));
    };
    let Some(c2) = semantic_dhm.get(right) else {
        return Err(HybridVmError::ConceptNotFound(right));
    };
    let query = ConceptQuery {
        v: c1.integrated_vector.clone(),
        a: c1.a,
        s: c1.s.clone(),
        polarity: c1.polarity,
    };
    let score = semantic_dhm::resonance(&query, &c2, semantic_dhm.weights());
    let v_sim = dot_norm(&c1.integrated_vector, &c2.integrated_vector);
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

pub(crate) fn explain_multiple(
    semantic_dhm: &SemanticDhm<FileStore<ConceptId, ConceptUnit>>,
    recomposer: &Recomposer,
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
        let Some(c) = semantic_dhm.get(id) else {
            return Err(HybridVmError::ConceptNotFound(id));
        };
        concepts.push(c);
    }
    let input = MultiConceptInput {
        concepts,
        weights: None,
    };
    Ok(recomposer.explain_multiple(&input, &semantic_dhm.weights()))
}

pub(crate) fn recommend(
    semantic_dhm: &SemanticDhm<FileStore<ConceptId, ConceptUnit>>,
    recomposer: &Recomposer,
    query_id: ConceptId,
    top_k: usize,
) -> Result<recomposer::RecommendationReport, HybridVmError> {
    let Some(query) = semantic_dhm.get(query_id) else {
        return Err(HybridVmError::ConceptNotFound(query_id));
    };
    let mut candidates = semantic_dhm
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
    Ok(recomposer.recommend(&input, &semantic_dhm.weights()))
}

pub(crate) fn design_report(
    semantic_dhm: &SemanticDhm<FileStore<ConceptId, ConceptUnit>>,
    recomposer: &Recomposer,
    concept_ids: &[ConceptId],
    top_k: usize,
) -> Result<DesignReport, HybridVmError> {
    let mut ids = dedup_ids(concept_ids);
    ids.sort_unstable();
    if ids.is_empty() {
        return Err(HybridVmError::InvalidInput(
            "report requires at least 1 concept id",
        ));
    }

    let mut concepts = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(c) = semantic_dhm.get(id) else {
            return Err(HybridVmError::ConceptNotFound(id));
        };
        concepts.push(c);
    }
    Ok(recomposer.generate_report(&concepts, &semantic_dhm.weights(), top_k))
}

pub(crate) fn decide(
    semantic_dhm: &SemanticDhm<FileStore<ConceptId, ConceptUnit>>,
    recomposer: &Recomposer,
    ids: &[ConceptId],
    weights: DecisionWeights,
) -> Result<DecisionReport, HybridVmError> {
    let ids = dedup_ids(ids);
    if ids.len() < 2 {
        return Err(HybridVmError::InvalidInput(
            "at least two concepts required",
        ));
    }
    let mut concepts = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(c) = semantic_dhm.get(id) else {
            return Err(HybridVmError::ConceptNotFound(id));
        };
        concepts.push(c);
    }
    recomposer
        .decide(&concepts, weights, &semantic_dhm.weights())
        .map_err(HybridVmError::Decision)
}

#[allow(dead_code)]
pub(crate) fn weights(
    semantic_dhm: &SemanticDhm<FileStore<ConceptId, ConceptUnit>>,
) -> ResonanceWeights {
    semantic_dhm.weights()
}
