use std::collections::VecDeque;
use std::sync::Mutex;

pub trait AgentClient {
    fn call(&self, prompt: String) -> Result<String, String>;
}

#[derive(Debug, Default)]
pub struct MockAgentClient {
    responses: Mutex<VecDeque<Result<String, String>>>,
}

impl MockAgentClient {
    pub fn new(responses: Vec<Result<String, String>>) -> Self {
        Self {
            responses: Mutex::new(responses.into()),
        }
    }

    pub fn from_json_responses(responses: Vec<String>) -> Self {
        Self::new(responses.into_iter().map(Ok).collect())
    }
}

impl AgentClient for MockAgentClient {
    fn call(&self, _prompt: String) -> Result<String, String> {
        self.responses
            .lock()
            .map_err(|_| "mock agent response lock poisoned".to_string())?
            .pop_front()
            .unwrap_or_else(|| Err("mock agent response exhausted".to_string()))
    }
}
