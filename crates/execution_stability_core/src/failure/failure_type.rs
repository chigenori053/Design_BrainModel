#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FailureType {
    DependencyFailure,
    BuildFailure,
    RuntimeFailure,
    TestFailure,
    Timeout,
    EnvironmentError,
}
