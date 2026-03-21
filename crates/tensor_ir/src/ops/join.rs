use std::collections::BTreeSet;

use crate::relation::{Provenance, Relation};

pub fn join(left: &[Relation], right: &[Relation]) -> Vec<Relation> {
    let mut joined = Vec::new();
    let mut seen = BTreeSet::new();

    for lhs in left {
        for rhs in right {
            if lhs.object != rhs.subject {
                continue;
            }

            let relation = Relation::new(
                lhs.subject,
                lhs.predicate,
                rhs.object,
                lhs.weight * rhs.weight,
                joined_provenance(lhs.provenance, rhs.provenance),
            );

            let key = (relation.subject, relation.predicate, relation.object);
            if seen.insert(key) {
                joined.push(relation);
            }
        }
    }

    joined
}

fn joined_provenance(left: Provenance, right: Provenance) -> Provenance {
    match (left, right) {
        (Provenance::LLMGenerated, _) | (_, Provenance::LLMGenerated) => Provenance::LLMGenerated,
        (Provenance::Inferred, _) | (_, Provenance::Inferred) => Provenance::Inferred,
        (Provenance::Symbolic, _) | (_, Provenance::Symbolic) => Provenance::Symbolic,
        _ => Provenance::Memory,
    }
}
