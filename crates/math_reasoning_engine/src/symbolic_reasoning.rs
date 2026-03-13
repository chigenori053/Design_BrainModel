use architecture_domain::ArchitectureState;

#[derive(Clone, Debug, PartialEq)]
pub struct SymbolicReasoningResult {
    pub summary: String,
    pub validity_score: f64,
}

pub trait SymbolicReasoner {
    fn reason(&self, architecture: &ArchitectureState) -> SymbolicReasoningResult;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct DeterministicSymbolicReasoner;

impl SymbolicReasoner for DeterministicSymbolicReasoner {
    fn reason(&self, architecture: &ArchitectureState) -> SymbolicReasoningResult {
        let summary = format!(
            "latency = dependency_count({}) + replicas({})",
            architecture.metrics.dependency_count,
            architecture.deployment.replicas
        );
        let validity_score = if architecture.metrics.layering_score >= 0.5 {
            0.9
        } else {
            0.6
        };
        SymbolicReasoningResult {
            summary,
            validity_score,
        }
    }
}
