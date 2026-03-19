use crate::engine::execution_plan::ExecutionPlan;
use crate::environment::runtime_env::RuntimeEnvironment;
use std::fs;
use std::path::PathBuf;

pub trait EnvironmentManager {
    fn prepare(&self, plan: &ExecutionPlan) -> Result<RuntimeEnvironment, String>;
    fn cleanup(&self) -> Result<(), String>;
}

#[derive(Clone, Debug, Default)]
pub struct LocalEnvironmentManager;

impl EnvironmentManager for LocalEnvironmentManager {
    fn prepare(&self, plan: &ExecutionPlan) -> Result<RuntimeEnvironment, String> {
        let execution_root = plan.project_root.join(".design_brainmodel_exec");
        fs::create_dir_all(&execution_root).map_err(|e| e.to_string())?;
        Ok(RuntimeEnvironment {
            working_directory: plan.project_root.clone(),
            execution_root,
        })
    }

    fn cleanup(&self) -> Result<(), String> {
        Ok(())
    }
}

pub fn normalize_working_directory(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}
