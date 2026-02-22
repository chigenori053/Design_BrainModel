use language_dhm::{EMBEDDING_DIM, LangId, LanguageDhm, LanguageUnit};
use memory_store::FileStore;
use semantic_dhm::{
    ConceptId, ConceptUnit, L1Id, RequirementRole, SemanticDhm, SemanticError, SemanticL1Dhm,
    SemanticUnitL1, SemanticUnitL1Input,
};

const ABS_PRECISION: f64 = 1000.0;
const ABSTRACTION_RULE_WEIGHT: f32 = 0.6;
const ABSTRACTION_VECTOR_WEIGHT: f32 = 0.4;

#[derive(Clone, Default)]
pub struct MeaningEngine;

impl MeaningEngine {
    pub fn analyze_text(
        &self,
        text: &str,
        language_dhm: &mut LanguageDhm<FileStore<LangId, LanguageUnit>>,
        semantic_l1_dhm: &mut SemanticL1Dhm<FileStore<L1Id, SemanticUnitL1>>,
        semantic_dhm: &mut SemanticDhm<FileStore<ConceptId, ConceptUnit>>,
    ) -> Result<ConceptUnit, SemanticError> {
        let embedding = self.embedding_from_text(text);
        let _ = language_dhm
            .insert(text, embedding)
            .map_err(|e| SemanticError::EvaluationError(e.to_string()))?;

        let fragments = self.extract_l1_fragments(text);
        let mut inserted = Vec::new();
        for fragment in fragments {
            let role = self.infer_requirement_role(&fragment);
            let l1_id = semantic_l1_dhm.insert(&SemanticUnitL1Input {
                role,
                polarity: self.infer_polarity(role),
                abstraction: self.infer_abstraction(&fragment),
                vector: self.embedding_from_text(&fragment),
                source_text: fragment,
            });
            let Some(unit) = semantic_l1_dhm.get(l1_id) else {
                return Err(SemanticError::InconsistentState("failed to persist l1 unit"));
            };
            inserted.push(unit);
        }

        semantic_dhm.rebuild_l2_from_l1(&semantic_l1_dhm.all_units())?;
        let mut candidates = semantic_dhm
            .all_concepts()
            .into_iter()
            .filter(|c| {
                inserted
                    .iter()
                    .any(|unit| c.l1_refs.binary_search(&unit.id).is_ok())
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|l, r| l.id.cmp(&r.id));
        candidates
            .into_iter()
            .next()
            .ok_or(SemanticError::InconsistentState(
                "no l2 generated from inserted l1",
            ))
    }

    pub fn embedding_from_text(&self, text: &str) -> Vec<f32> {
        let mut out = vec![0.0f32; EMBEDDING_DIM];
        for (i, b) in text.bytes().enumerate() {
            let idx = (i.saturating_mul(131).saturating_add(b as usize)) % out.len();
            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
            let value = (b as f32 / 255.0) - 0.5;
            out[idx] += sign * value;
        }
        out
    }

    pub fn extract_l1_fragments(&self, text: &str) -> Vec<String> {
        let mut cleaned = text.replace('\n', " ");
        for sep in [
            "。",
            "、",
            ",",
            ";",
            " and ",
            " but ",
            " しかし ",
            " ただし ",
            " また ",
        ] {
            cleaned = cleaned.replace(sep, "|");
        }
        let mut out = cleaned
            .split('|')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>();
        if out.is_empty() {
            out.push(text.trim().to_string());
        }
        out
    }

    pub fn infer_requirement_role(&self, text: &str) -> RequirementRole {
        let t = text.to_ascii_lowercase();
        if t.contains("avoid")
            || t.contains("prohibit")
            || t.contains("forbid")
            || t.contains("禁止")
            || t.contains("避け")
        {
            RequirementRole::Prohibition
        } else if t.contains("must")
            || t.contains("以下")
            || t.contains("上限")
            || t.contains("constraint")
            || t.contains("制約")
        {
            RequirementRole::Constraint
        } else if t.contains("optimiz")
            || t.contains("best")
            || t.contains("できるだけ")
            || t.contains("省エネ")
        {
            RequirementRole::Optimization
        } else {
            RequirementRole::Goal
        }
    }

    pub fn infer_polarity(&self, role: RequirementRole) -> i8 {
        match role {
            RequirementRole::Goal | RequirementRole::Optimization => 1,
            RequirementRole::Constraint | RequirementRole::Prohibition => -1,
        }
    }

    pub fn infer_abstraction(&self, text: &str) -> f32 {
        let rule_score = self.rule_abstraction_score(text);
        let vector_score = self.vector_abstraction_score(text);
        let mixed = ABSTRACTION_RULE_WEIGHT * rule_score + ABSTRACTION_VECTOR_WEIGHT * vector_score;
        self.quantize_abstraction(mixed.clamp(0.0, 1.0))
    }

    fn rule_abstraction_score(&self, text: &str) -> f32 {
        let t = text.to_ascii_lowercase();
        let mut score = 0.5f32;

        if text.chars().any(|c| c.is_ascii_digit()) {
            score -= 0.4;
        }
        if ["mb", "gb", "kb", "ms", "%", "件", "秒", "回"]
            .iter()
            .any(|u| t.contains(u) || text.contains(u))
        {
            score -= 0.3;
        }
        if [
            "以下",
            "以上",
            "未満",
            "以内",
            "must",
            "limit",
            "constraint",
            "禁止",
            "avoid",
            "forbid",
            "prohibit",
        ]
        .iter()
        .any(|w| t.contains(w) || text.contains(w))
        {
            score -= 0.3;
        }
        if [
            "できるだけ",
            "なるべく",
            "as much as possible",
            "preferably",
        ]
        .iter()
        .any(|w| t.contains(w) || text.contains(w))
        {
            score += 0.3;
        }
        if [
            "性能",
            "安全",
            "品質",
            "performance",
            "security",
            "quality",
            "高性能",
            "高速",
            "速く",
            "fast",
        ]
        .iter()
        .any(|w| t.contains(w) || text.contains(w))
        {
            score += 0.2;
        }

        score.clamp(0.0, 1.0)
    }

    fn vector_abstraction_score(&self, text: &str) -> f32 {
        let v = self.embedding_from_text(text);
        let mu = self.generic_center_vector(v.len());
        let cosine = dot_norm(&v, &mu).clamp(-1.0, 1.0);
        let d = 1.0 - cosine;
        (d / 2.0).clamp(0.0, 1.0)
    }

    fn generic_center_vector(&self, dim: usize) -> Vec<f32> {
        (0..dim)
            .map(|i| {
                let phase = ((i as f32 + 1.0) * 0.173_205_08).sin();
                let bias = ((i as f32 + 1.0) * 0.031_25).cos() * 0.15;
                phase + bias
            })
            .collect()
    }

    fn quantize_abstraction(&self, v: f32) -> f32 {
        ((v as f64 * ABS_PRECISION).round() / ABS_PRECISION) as f32
    }
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
