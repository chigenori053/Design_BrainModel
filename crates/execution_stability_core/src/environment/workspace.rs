use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workspace {
    pub execution_id: String,
    pub root_dir: PathBuf,
    pub project_root: PathBuf,
}
