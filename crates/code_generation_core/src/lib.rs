use architecture_domain::ArchitectureState;
use execution_core::engine::execution_plan::ExecutionPlan;
use execution_core::engine::execution_result::ExecutionResult;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodeArtifact {
    pub files: Vec<GeneratedFile>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeneratedFile {
    pub path: String,
    pub contents: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectLayout {
    pub root: String,
    pub directories: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeResult {
    pub files: Vec<GeneratedFile>,
    pub project_layout: ProjectLayout,
    pub execution_plan: ExecutionPlan,
    pub execution_result: Option<ExecutionResult>,
}

pub trait CodeGenerator {
    fn generate_code(&self, architecture: &ArchitectureState) -> CodeArtifact;
}
