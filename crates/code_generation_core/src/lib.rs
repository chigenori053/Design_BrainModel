use architecture_domain::ArchitectureState;

pub mod dep_graph;
pub mod emit;
pub mod error;
pub mod generator;
pub mod project;
pub mod scope;
pub mod spec;
pub mod type_render;

#[cfg(test)]
mod tests;

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
