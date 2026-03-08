#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidationIssue {
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GrammarValidation {
    pub valid: bool,
    pub issues: Vec<ValidationIssue>,
}

impl GrammarValidation {
    pub fn from_messages(messages: Vec<String>) -> Self {
        let issues = messages
            .into_iter()
            .map(|message| ValidationIssue { message })
            .collect::<Vec<_>>();
        Self {
            valid: issues.is_empty(),
            issues,
        }
    }
}
