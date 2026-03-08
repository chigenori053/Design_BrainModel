#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArchitectureGraph {
    pub edges: Vec<(u64, u64)>,
}
