use concept_engine::ConceptId;
use reasoning_agent::hypothesis::generate_bound_concept_pairs;

#[test]
fn reasoning_generates_concept_hypothesis() {
    let concept_a = ConceptId::from_name("DATABASE");
    let concept_b = ConceptId::from_name("CACHE");

    let hypotheses = generate_bound_concept_pairs(&[concept_a, concept_b], 4);

    assert!(!hypotheses.is_empty());
    assert_eq!(
        hypotheses[0],
        (concept_a.min(concept_b), concept_a.max(concept_b))
    );
}

trait ConceptOrder {
    fn min(self, rhs: Self) -> Self;
    fn max(self, rhs: Self) -> Self;
}

impl ConceptOrder for ConceptId {
    fn min(self, rhs: Self) -> Self {
        if self.0 <= rhs.0 { self } else { rhs }
    }

    fn max(self, rhs: Self) -> Self {
        if self.0 >= rhs.0 { self } else { rhs }
    }
}
