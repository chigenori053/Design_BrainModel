#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum LanguageAction {
    InferIntent,
    ExpandConcept,
    AddRelation,
    ResolveAmbiguity,
    GenerateSentence,
}
