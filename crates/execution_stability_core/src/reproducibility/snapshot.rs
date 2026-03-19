use crate::reproducibility::lock_manager::LockManager;
use execution_core::engine::execution_plan::TargetLanguage;
use execution_core::environment::toolchain::toolchain_for;
use std::path::Path;
use std::process::Command;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionSnapshot {
    pub language: TargetLanguage,
    pub toolchain_version: String,
    pub lockfile_hash: String,
    pub os_type: String,
    pub architecture: String,
    pub env_vars: Vec<(String, String)>,
    pub working_dir_hash: String,
}

#[derive(Clone, Debug, Default)]
pub struct ReproducibilityManager {
    pub lock_manager: LockManager,
}

impl ReproducibilityManager {
    pub fn snapshot(
        &self,
        language: &TargetLanguage,
        project_root: &Path,
        working_dir_hash: String,
    ) -> Result<ExecutionSnapshot, String> {
        let toolchain_version = detect_toolchain_version(language)?;
        let lockfile_hash = self.lock_manager.lockfile_hash(project_root)?;
        let mut env_vars = std::env::vars()
            .filter(|(key, _)| matches!(key.as_str(), "LANG" | "LC_ALL" | "PATH" | "HOME"))
            .collect::<Vec<_>>();
        env_vars.sort();
        Ok(ExecutionSnapshot {
            language: language.clone(),
            toolchain_version,
            lockfile_hash,
            os_type: std::env::consts::OS.to_string(),
            architecture: std::env::consts::ARCH.to_string(),
            env_vars,
            working_dir_hash,
        })
    }
}

fn detect_toolchain_version(language: &TargetLanguage) -> Result<String, String> {
    let toolchain = toolchain_for(language);
    let program = match language {
        TargetLanguage::Rust => toolchain.build_tool,
        TargetLanguage::Python => "python3",
        TargetLanguage::TypeScript => toolchain.run_tool,
        TargetLanguage::Other(_) => toolchain.run_tool,
    };
    let output = Command::new(program)
        .arg("--version")
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        Ok(String::from_utf8_lossy(&output.stderr).trim().to_string())
    } else {
        Ok(stdout)
    }
}
