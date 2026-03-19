use crate::engine::execution_plan::DependencyPlan;
use std::fs;
use std::path::Path;

pub fn write_manifest(project_root: &Path, plan: &DependencyPlan) -> Result<(), String> {
    let content = plan
        .dependencies
        .iter()
        .map(|dep| {
            if let Some(version) = &dep.version {
                format!("{}={}", dep.name, version)
            } else {
                dep.name.clone()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(project_root.join(&plan.manifest_file), content).map_err(|e| e.to_string())
}
