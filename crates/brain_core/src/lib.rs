pub mod ast;
pub mod decision;
pub mod memory;
pub mod reasoning;
pub mod semantic;
pub mod tensor;
pub mod validation;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoreState {
    pub tick: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreError {
    pub message: &'static str,
}

pub type CoreResult<T> = Result<T, CoreError>;

pub trait MemoryStore {
    fn load(&self, key: &str) -> CoreResult<Option<String>>;
    fn save(&mut self, key: &str, value: String) -> CoreResult<()>;
}

pub trait Logger {
    fn log(&mut self, message: &str) -> CoreResult<()>;
}

pub trait ExecutionContext {
    fn now_tick(&self) -> u64;
}

#[derive(Debug, Clone, Default)]
pub struct ReasoningEngine;

impl ReasoningEngine {
    pub fn step(&self, state: &CoreState) -> CoreResult<CoreState> {
        Ok(CoreState { tick: state.tick + 1 })
    }
}
