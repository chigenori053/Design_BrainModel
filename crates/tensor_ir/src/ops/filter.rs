use crate::relation::Relation;

pub fn filter(relations: &[Relation], threshold: f32) -> Vec<Relation> {
    relations
        .iter()
        .filter(|relation| relation.weight >= threshold)
        .cloned()
        .collect()
}
