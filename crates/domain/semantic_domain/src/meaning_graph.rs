use crate::{Intent, SemanticRelation, SemanticUnit};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MeaningGraph {
    pub intents: Vec<Intent>,
    pub semantic_units: Vec<SemanticUnit>,
    pub relations: Vec<SemanticRelation>,
}
