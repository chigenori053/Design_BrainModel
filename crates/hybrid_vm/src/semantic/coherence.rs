pub type SemanticUnitL2V2 = semantic_dhm::ConceptUnitV2;

const EPS: f64 = 1e-12;
const HC_EXPONENT: f64 = 1.2;
const COVERAGE_EXPONENT: f64 = 0.5;
const CONSISTENCY_EXPONENT: f64 = 1.0;
const DEP_QUALITY_EXPONENT: f64 = 1.0;
const ALIGNMENT_BLEND: f64 = 0.35;
const STRUCTURAL_BLEND: f64 = 0.65;
const HC_OUTPUT_EXPONENT: f64 = 1.4;

#[derive(Clone, Debug, PartialEq)]
pub struct HumanCoherence {
    pub coverage: f64,
    pub consistency: f64,
    pub dependency_quality: f64,
    pub raw_score: f64,
    pub score: f64,
}

pub fn compute_human_coherence(l2: &SemanticUnitL2V2) -> HumanCoherence {
    let coverage = slot_coverage(l2);
    let consistency = structural_consistency(l2);
    let dependency_quality = dependency_structure_quality(l2);
    let (_structural_raw, structural_score) =
        human_coherence_from_components(coverage, consistency, dependency_quality);
    let alignment = requirement_alignment_mean(l2);
    let raw_score = clamp01(ALIGNMENT_BLEND * alignment + STRUCTURAL_BLEND * structural_score);
    let score = clamp01(raw_score.powf(HC_OUTPUT_EXPONENT));

    HumanCoherence {
        coverage,
        consistency,
        dependency_quality,
        raw_score,
        score,
    }
}

pub fn human_coherence_from_components(
    coverage: f64,
    consistency: f64,
    dependency_quality: f64,
) -> (f64, f64) {
    let raw_score = clamp01(
        coverage.powf(COVERAGE_EXPONENT)
            * consistency.powf(CONSISTENCY_EXPONENT)
            * dependency_quality.powf(DEP_QUALITY_EXPONENT),
    );
    let score = clamp01(raw_score.powf(HC_EXPONENT));
    (raw_score, score)
}

fn slot_coverage(l2: &SemanticUnitL2V2) -> f64 {
    use semantic_dhm::RequirementKind::{Memory, NoCloud, Performance, Reliability, Security};
    let has_kind = |k| l2.derived_requirements.iter().any(|r| r.kind == k);
    let has_any_derived = !l2.derived_requirements.is_empty();
    let has_any_edges = !l2.causal_links.is_empty();
    let who = has_any_derived || has_any_edges;
    let what = has_any_derived;
    let why = has_kind(Reliability);
    let where_ = has_kind(NoCloud) || has_kind(Memory);
    let when_ = has_any_edges;
    let how = has_kind(Performance);
    let how_much = has_kind(Performance) && has_kind(Memory);
    let success_metric = has_kind(Security) || has_kind(Reliability);
    let filled = [who, what, why, where_, when_, how, how_much, success_metric]
        .into_iter()
        .filter(|v| *v)
        .count() as f64;
    clamp01(filled / 8.0)
}

fn structural_consistency(l2: &SemanticUnitL2V2) -> f64 {
    if l2.causal_links.len() <= 1 {
        return 0.5;
    }
    clamp01(1.0 - sign_transition_density(l2))
}

fn dependency_structure_quality(l2: &SemanticUnitL2V2) -> f64 {
    let concentration = edge_weight_concentration(l2);
    let variance = requirement_variance(l2);
    clamp01(1.0 - 0.6 * concentration - 0.4 * variance)
}

fn sign_transition_density(l2: &SemanticUnitL2V2) -> f64 {
    if l2.causal_links.len() <= 1 {
        return 0.0;
    }
    let mut edges = l2.causal_links.to_vec();
    edges.sort_by(|a, b| (a.from, a.to).cmp(&(b.from, b.to)));
    let mut flips = 0usize;
    let mut pairs = 0usize;
    for w in edges.windows(2) {
        let a = w[0].weight;
        let b = w[1].weight;
        pairs += 1;
        if a * b < 0.0 {
            flips += 1;
        }
    }
    if pairs == 0 {
        0.0
    } else {
        flips as f64 / pairs as f64
    }
}

fn edge_weight_concentration(l2: &SemanticUnitL2V2) -> f64 {
    if l2.causal_links.is_empty() {
        return 0.5;
    }
    let abs = l2
        .causal_links
        .iter()
        .map(|e| e.weight.abs().clamp(0.0, 1.0))
        .collect::<Vec<_>>();
    let sum = abs.iter().sum::<f64>();
    if sum <= EPS {
        return 1.0;
    }
    let max = abs.iter().copied().fold(0.0_f64, f64::max);
    clamp01(max / sum)
}

fn requirement_variance(l2: &SemanticUnitL2V2) -> f64 {
    if l2.derived_requirements.is_empty() {
        return 0.5;
    }
    let levels = l2
        .derived_requirements
        .iter()
        .map(|r| ((r.strength as f64).clamp(-1.0, 1.0) + 1.0) * 0.5)
        .collect::<Vec<_>>();
    let mean = levels.iter().sum::<f64>() / levels.len() as f64;
    let var = levels.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / levels.len() as f64;
    clamp01(var / 0.25)
}

fn requirement_alignment_mean(l2: &SemanticUnitL2V2) -> f64 {
    if l2.derived_requirements.is_empty() {
        return 0.5;
    }
    let levels = l2
        .derived_requirements
        .iter()
        .map(|r| ((r.strength as f64).clamp(-1.0, 1.0) + 1.0) * 0.5)
        .collect::<Vec<_>>();
    clamp01(levels.iter().sum::<f64>() / levels.len() as f64)
}

fn clamp01(v: f64) -> f64 {
    if v.is_nan() {
        return 0.0;
    }
    v.clamp(0.0, 1.0)
}
