#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionResult {
    pub success: bool,
    pub dependency_result: StepResult,
    pub build_result: StepResult,
    pub run_result: StepResult,
    pub test_result: StepResult,
    pub logs: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StepResult {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl StepResult {
    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            success: true,
            stdout: String::new(),
            stderr: reason.into(),
        }
    }
}
