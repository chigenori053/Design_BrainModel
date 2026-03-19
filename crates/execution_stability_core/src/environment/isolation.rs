use crate::environment::sandbox::Sandbox;
use crate::environment::workspace::Workspace;
use execution_core::engine::execution_plan::ExecutionPlan;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static EXECUTION_COUNTER: AtomicU64 = AtomicU64::new(0);

pub trait EnvironmentManager: Send + Sync {
    fn prepare_isolated(&self, plan: &ExecutionPlan) -> Result<Workspace, String>;
    fn cleanup(&self, workspace: &Workspace) -> Result<(), String>;
}

#[derive(Clone, Debug)]
pub struct IsolatedEnvironmentManager {
    pub base_dir: PathBuf,
    pub sandbox: Sandbox,
    pub cleanup_on_drop: bool,
}

impl Default for IsolatedEnvironmentManager {
    fn default() -> Self {
        Self {
            base_dir: std::env::temp_dir().join("dbm"),
            sandbox: Sandbox::default(),
            cleanup_on_drop: true,
        }
    }
}

impl EnvironmentManager for IsolatedEnvironmentManager {
    fn prepare_isolated(&self, plan: &ExecutionPlan) -> Result<Workspace, String> {
        let execution_id = format!("exec-{}", unique_nanos()?);
        let root_dir = self.base_dir.join(&execution_id);
        let project_root = root_dir.join("project");
        fs::create_dir_all(&project_root).map_err(|error| error.to_string())?;
        copy_tree(&plan.project_root, &project_root)?;
        Ok(Workspace {
            execution_id,
            root_dir,
            project_root,
        })
    }

    fn cleanup(&self, workspace: &Workspace) -> Result<(), String> {
        if self.cleanup_on_drop && workspace.root_dir.exists() {
            fs::remove_dir_all(&workspace.root_dir).map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

fn copy_tree(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.exists() {
        return Err(format!("project root does not exist: {}", source.display()));
    }
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_dir() {
            fs::create_dir_all(&destination_path).map_err(|error| error.to_string())?;
            copy_tree(&source_path, &destination_path)?;
        } else {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::copy(&source_path, &destination_path).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn unique_nanos() -> Result<u128, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| {
            let counter = EXECUTION_COUNTER.fetch_add(1, Ordering::Relaxed) as u128;
            duration.as_nanos() * 1_000 + counter
        })
        .map_err(|error| error.to_string())
}
