use crate::environment::workspace::Workspace;
use execution_core::engine::execution_result::StepResult;
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Container {
    pub id: String,
    pub base_image: String,
    pub immutable: bool,
    pub workspace_root: PathBuf,
    pub fallback_local: bool,
}

pub trait ContainerManager: Send + Sync {
    fn create_container(
        &self,
        snapshot: &crate::reproducibility::snapshot::ExecutionSnapshot,
        workspace: &Workspace,
    ) -> Result<Container, String>;
    fn execute_in_container(&self, container: &Container, cmd: &[String]) -> StepResult;
    fn destroy_container(&self, container: Container) -> Result<(), String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultContainerManager {
    pub base_image: String,
    pub use_real_container: bool,
}

impl Default for DefaultContainerManager {
    fn default() -> Self {
        Self {
            base_image: "dbm/stable-v03:locked".to_string(),
            use_real_container: false,
        }
    }
}

impl ContainerManager for DefaultContainerManager {
    fn create_container(
        &self,
        snapshot: &crate::reproducibility::snapshot::ExecutionSnapshot,
        workspace: &Workspace,
    ) -> Result<Container, String> {
        Ok(Container {
            id: format!("{}-{}", workspace.execution_id, snapshot.lockfile_hash),
            base_image: self.base_image.clone(),
            immutable: true,
            workspace_root: workspace.project_root.clone(),
            fallback_local: !self.use_real_container,
        })
    }

    fn execute_in_container(&self, container: &Container, cmd: &[String]) -> StepResult {
        let Some((program, args)) = cmd.split_first() else {
            return StepResult {
                success: false,
                stdout: String::new(),
                stderr: "container command must not be empty".to_string(),
            };
        };
        let output = Command::new(program)
            .args(args)
            .current_dir(&container.workspace_root)
            .output();
        match output {
            Ok(output) => StepResult {
                success: output.status.success(),
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            },
            Err(error) => StepResult {
                success: false,
                stdout: String::new(),
                stderr: error.to_string(),
            },
        }
    }

    fn destroy_container(&self, _container: Container) -> Result<(), String> {
        Ok(())
    }
}
