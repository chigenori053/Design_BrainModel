use crate::capability::SearchHit;
use crate::domain::DomainError;
use crate::ports::SearchPort;

#[derive(Clone, Debug, Default)]
pub struct HttpClient;

impl SearchPort for HttpClient {
    fn search(&self, _query: &str) -> Result<Vec<SearchHit>, DomainError> {
        Err(DomainError::Unsupported(
            "http search adapter is not wired yet".to_string(),
        ))
    }
}
