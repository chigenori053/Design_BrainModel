#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeterminismReport {
    pub is_deterministic: bool,
    pub diff: Option<String>,
}
