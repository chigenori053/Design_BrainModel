use crate::DesignUnitId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DependencyKind {
    Calls,
    Reads,
    Writes,
    Emits,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dependency {
    pub from: DesignUnitId,
    pub to: DesignUnitId,
    pub kind: DependencyKind,
}
