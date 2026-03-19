#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StepTrace {
    pub step_name: String,
    pub command: Vec<String>,
    pub start_time: u64,
    pub end_time: u64,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}
