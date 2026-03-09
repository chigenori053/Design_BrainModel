use architecture_domain::ArchitectureState;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodeArtifact {
    pub files: Vec<GeneratedFile>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GeneratedFile {
    pub path: String,
    pub contents: String,
}

pub trait CodeGenerator {
    fn generate_code(&self, architecture: &ArchitectureState) -> CodeArtifact;
}
