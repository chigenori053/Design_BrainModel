use std::path::{Component, Path};
//use runtime_vm::adapter_world_interface::AdapterWorldInterface;
use crate::dbm::{DBMClient, ProjectAnalysisResult};
pub(crate) fn analyze_project(path: &Path) -> Result<ProjectAnalysisResult, String> {
    if !path.exists() {
        return Err(format!("path does not exist: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("path is not a directory: {}", path.display()));
    }

    DBMClient::new()
        .analyze_project(&path.display().to_string())
        .map_err(|err| format!("project analysis failed: {err}"))
}

pub(crate) fn path_contains_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}
