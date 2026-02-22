use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{Display, Formatter};

use crate::domain::event::AgentEvent;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentInput {
    pub text: String,
    pub metadata: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct AgentOutput {
    pub summary: String,
    pub artifacts: Vec<String>,
    pub events: Vec<AgentEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRequest {
    pub target: String,
    pub input: AgentInput,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DomainError {
    AgentNotFound(String),
    InvalidInput(String),
    PortError(String),
    Unsupported(String),
    Internal(String),
}

impl Display for DomainError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AgentNotFound(name) => write!(f, "agent not found: {name}"),
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            Self::PortError(msg) => write!(f, "port error: {msg}"),
            Self::Unsupported(msg) => write!(f, "unsupported: {msg}"),
            Self::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl Error for DomainError {}
