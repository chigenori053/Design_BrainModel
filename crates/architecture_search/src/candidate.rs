use crate::ArchitectureScore;

#[derive(Clone, Debug, PartialEq)]
pub struct ArchitectureCandidate {
    pub architecture_ir: architecture_ir::ArchitectureIR,
    pub evaluation: ArchitectureScore,
    pub generation_step: usize,
}
