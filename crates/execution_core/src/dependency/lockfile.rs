use crate::engine::execution_plan::DependencyPlan;
use std::fs;
use std::path::Path;

pub fn write_lockfile(project_root: &Path, plan: &DependencyPlan) -> Result<(), String> {
    let lockfile = plan
        .dependencies
        .iter()
        .map(|dep| {
            format!(
                "{}:{}",
                dep.name,
                dep.version.clone().unwrap_or_else(|| "*".into())
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(project_root.join("execution.lock"), lockfile).map_err(|e| e.to_string())
}
