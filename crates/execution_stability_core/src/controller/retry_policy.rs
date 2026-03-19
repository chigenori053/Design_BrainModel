use crate::failure::failure_type::FailureType;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub retry_on: Vec<FailureType>,
}

impl RetryPolicy {
    pub fn should_retry(&self, failure: &FailureType, attempt: u32) -> bool {
        attempt < self.max_retries && self.retry_on.iter().any(|kind| kind == failure)
    }
}
