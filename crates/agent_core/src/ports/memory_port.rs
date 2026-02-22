use crate::domain::DomainError;

pub trait MemoryPort: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<Vec<u8>>, DomainError>;
    fn put(&self, key: &str, value: &[u8]) -> Result<(), DomainError>;
}
