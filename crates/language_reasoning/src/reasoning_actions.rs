#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum ReasoningAction {
    InferConstraint,
    InferArchitecturePattern,
    InferDependency,
    ExpandConcept,
    ResolveAmbiguity,
}
