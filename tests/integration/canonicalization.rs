use concept_engine::{Canonicalizer, ConceptRegistry};

fn emb(values: [f32; 4]) -> Vec<f32> {
    values.to_vec()
}

#[test]
fn canonicalization_merges_semantic_duplicates() {
    let mut canonicalizer = Canonicalizer::new(ConceptRegistry::default());

    let c1 = canonicalizer.canonicalize("optimize query", &emb([1.0, 0.0, 0.0, 0.0]));
    let c2 = canonicalizer.canonicalize("query optimization", &emb([0.99, 0.01, 0.0, 0.0]));
    let c3 = canonicalizer.canonicalize("improve query performance", &emb([0.98, 0.02, 0.0, 0.0]));

    assert_eq!(c1, c2);
    assert_eq!(c2, c3);
}
