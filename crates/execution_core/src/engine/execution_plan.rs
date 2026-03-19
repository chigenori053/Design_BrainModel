use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TargetLanguage {
    Rust,
    Python,
    TypeScript,
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionPlan {
    pub language: TargetLanguage,
    pub framework: Option<String>,
    pub project_root: PathBuf,
    pub dependency_plan: DependencyPlan,
    pub build_plan: BuildPlan,
    pub run_plan: RunPlan,
    pub test_plan: TestPlan,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DependencyPlan {
    pub manifest_file: String,
    pub dependencies: Vec<DependencySpec>,
    pub install_commands: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DependencySpec {
    pub name: String,
    pub version: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuildPlan {
    pub build_commands: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunPlan {
    pub run_commands: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TestPlan {
    pub test_files: Vec<String>,
    pub test_commands: Vec<String>,
}
