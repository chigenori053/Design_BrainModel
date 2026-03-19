use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeEnvironment {
    pub working_directory: PathBuf,
    pub execution_root: PathBuf,
}
