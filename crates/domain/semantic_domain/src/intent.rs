use crate::Concept;

#[derive(Clone, Debug, PartialEq)]
pub struct Intent {
    pub name: String,
    pub concepts: Vec<Concept>,
}
