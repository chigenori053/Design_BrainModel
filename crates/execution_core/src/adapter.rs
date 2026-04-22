use crate::engine::ExecutionResult;

pub trait ExecutionAdapter {
    fn on_result(&self, result: &ExecutionResult);
    fn on_error(&self, message: &str);
}

pub struct NoOpAdapter;

impl ExecutionAdapter for NoOpAdapter {
    fn on_result(&self, _result: &ExecutionResult) {}
    fn on_error(&self, _message: &str) {}
}
