use field_engine::{FieldEngine, FieldVector, NodeCategory, TargetField};
use hybrid_vm::{HybridVM, RuleCategory, Shm};
use memory_space::DesignState;

use crate::diversity;
use crate::diversity::apply_diversity_pressure;

pub fn build_target_field(
    field: &FieldEngine,
    shm: &Shm,
    state: &DesignState,
    lambda: f64,
) -> TargetField {
    let (target, _) = build_target_field_with_diversity(field, shm, state, lambda, 1.0);
    target
}

pub fn build_target_field_with_diversity(
    field: &FieldEngine,
    shm: &Shm,
    state: &DesignState,
    lambda: f64,
    diversity: f64,
) -> (TargetField, diversity::DiversityAdjustment) {
    let global_categories =
        categories_from_rules(HybridVM::rules(shm).iter().map(|r| r.category.clone()));
    let local_categories = categories_from_rules(
        HybridVM::applicable_rules(shm, state)
            .into_iter()
            .map(|rule| rule.category.clone()),
    );

    let global = compose_category_field(field, &global_categories);
    let local = compose_category_field(field, &local_categories);
    let base = TargetField::blend(&global, &local, lambda as f32);
    apply_diversity_pressure(&base, &global, &local, lambda, diversity)
}

fn categories_from_rules<I>(categories: I) -> Vec<NodeCategory>
where
    I: IntoIterator<Item = RuleCategory>,
{
    let mut out = Vec::new();
    for c in categories {
        let mapped = match c {
            RuleCategory::Structural => NodeCategory::Abstraction,
            RuleCategory::Performance => NodeCategory::Performance,
            RuleCategory::Reliability => NodeCategory::Reliability,
            RuleCategory::Cost => NodeCategory::CostSensitive,
            RuleCategory::Refactor => NodeCategory::Control,
            RuleCategory::ConstraintPropagation => NodeCategory::Constraint,
        };
        if !out.contains(&mapped) {
            out.push(mapped);
        }
    }
    out
}

fn compose_category_field(field: &FieldEngine, categories: &[NodeCategory]) -> FieldVector {
    if categories.is_empty() {
        return FieldVector::zeros(field.dimensions());
    }
    let basis = categories
        .iter()
        .map(|c| field.projector().basis_for(*c))
        .collect::<Vec<_>>();
    FieldVector::average(&basis, field.dimensions())
}
