#[derive(Debug, Clone)]
pub struct ValidationCheck {
    pub name: String,
    pub required: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    pub passed: bool,
    pub failures: Vec<String>,
}

impl ValidationResult {
    pub fn ok() -> Self {
        Self {
            passed: true,
            failures: vec![],
        }
    }
}

pub trait ValidateEngine {
    fn validate(&self, checks: &[ValidationCheck]) -> ValidationResult;
}
