use crate::domain::DomainError;

pub trait SimulationCapability: Send + Sync {
    fn simulate(&self, input: &str) -> Result<String, DomainError>;
}
