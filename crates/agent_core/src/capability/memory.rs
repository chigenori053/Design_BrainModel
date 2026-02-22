use crate::domain::DomainError;

pub trait MemoryCapability: Send + Sync {
    fn load(&self, key: &str) -> Result<Option<Vec<u8>>, DomainError>;
    fn store(&self, key: &str, value: &[u8]) -> Result<(), DomainError>;
}
