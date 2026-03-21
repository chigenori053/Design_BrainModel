use crate::relation::{Provenance, Relation};

pub fn compose(left: &Relation, right: &Relation) -> Option<Relation> {
    if left.predicate != right.predicate || left.object != right.subject {
        return None;
    }

    Some(Relation::new(
        left.subject,
        left.predicate,
        right.object,
        left.weight * right.weight,
        composed_provenance(left.provenance, right.provenance),
    ))
}

fn composed_provenance(left: Provenance, right: Provenance) -> Provenance {
    match (left, right) {
        (Provenance::LLMGenerated, _) | (_, Provenance::LLMGenerated) => Provenance::LLMGenerated,
        (Provenance::Inferred, _) | (_, Provenance::Inferred) => Provenance::Inferred,
        (Provenance::Symbolic, _) | (_, Provenance::Symbolic) => Provenance::Symbolic,
        _ => Provenance::Memory,
    }
}
