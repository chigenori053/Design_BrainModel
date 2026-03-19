use crate::trace::step_trace::StepTrace;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionTrace {
    pub execution_id: String,
    pub steps: Vec<StepTrace>,
}
