use semantic_dhm::{DerivedRequirement, DesignProjection, RequirementKind, SemanticError};

const SCORE_PRECISION: f64 = 1000.0;

#[derive(Clone, Debug, PartialEq)]
pub struct DesignHypothesis {
    pub requirements: Vec<DerivedRequirement>,
    pub total_score: f64,
    pub normalized_score: f64,
    pub constraint_violation: bool,
}

impl DesignHypothesis {
    pub fn dominant_requirement(&self) -> Option<RequirementKind> {
        self.requirements
            .iter()
            .max_by(|l, r| l.strength.abs().total_cmp(&r.strength.abs()))
            .map(|d| d.kind)
    }
}

#[derive(Clone, Default)]
pub struct HypothesisEngine;

impl HypothesisEngine {
    pub fn evaluate_hypothesis(
        &self,
        projection: &DesignProjection,
    ) -> Result<DesignHypothesis, SemanticError> {
        if projection.derived.is_empty() {
            return Err(SemanticError::InvalidInput(
                "derived requirements are empty".to_string(),
            ));
        }
        let strengths = projection
            .derived
            .iter()
            .map(|d| f64::from(d.strength))
            .collect::<Vec<_>>();
        let total = strengths.iter().copied().sum::<f64>();
        let denom = strengths.iter().map(|s| s.abs()).sum::<f64>();
        let normalized = if denom > 0.0 { total / denom } else { 0.0 };

        let constraint_violation = projection
            .derived
            .iter()
            .any(|d| is_constraint_kind(d.kind) && d.strength > 0.0);

        Ok(DesignHypothesis {
            requirements: projection.derived.clone(),
            total_score: quantize_score(total),
            normalized_score: quantize_score(normalized),
            constraint_violation,
        })
    }
}

fn quantize_score(v: f64) -> f64 {
    (v * SCORE_PRECISION).round() / SCORE_PRECISION
}

fn is_constraint_kind(kind: RequirementKind) -> bool {
    matches!(kind, RequirementKind::Memory | RequirementKind::NoCloud)
}
