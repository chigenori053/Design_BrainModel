use crate::domain::DomainError;

pub trait StoragePort: Send + Sync {
    fn write(&self, key: &str, value: &[u8]) -> Result<(), DomainError>;
    fn read(&self, key: &str) -> Result<Option<Vec<u8>>, DomainError>;

    fn write_trace(&self, key: &str, lines: &[String]) -> Result<(), DomainError> {
        let payload = if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        };
        self.write(key, payload.as_bytes())
    }
}
