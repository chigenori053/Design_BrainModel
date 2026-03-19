use crate::failure::failure_type::FailureType;

#[derive(Clone, Debug, Default)]
pub struct FailureAnalyzer;

impl FailureAnalyzer {
    pub fn classify_step_failure(
        &self,
        phase: &str,
        _stderr: &str,
        timed_out: bool,
        environment_error: bool,
    ) -> FailureType {
        if timed_out {
            return FailureType::Timeout;
        }
        if environment_error {
            return FailureType::EnvironmentError;
        }
        match phase {
            "dependency" => FailureType::DependencyFailure,
            "build" => FailureType::BuildFailure,
            "run" => FailureType::RuntimeFailure,
            "test" => FailureType::TestFailure,
            _ => FailureType::EnvironmentError,
        }
    }
}
