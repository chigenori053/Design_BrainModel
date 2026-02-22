use crate::capability::SearchHit;
use crate::domain::DomainError;

pub trait SearchPort: Send + Sync {
    fn search(&self, query: &str) -> Result<Vec<SearchHit>, DomainError>;
}
