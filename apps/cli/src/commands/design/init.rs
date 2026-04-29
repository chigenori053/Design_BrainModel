use std::fs;

use crate::command::{CommandError, Output, SubCommandHandler};
use crate::session::AgentSession;

use super::{design_file, design_to_yaml, make_initial_version, resolve_root, save_baseline};
use unified_design_ir::{
    ArchitectureSpec, ContextSpec, DataSpec, DesignDocument, ExecutionSpec, FunctionSpec, Metadata,
    Stage, VersionInterfaceSpec,
};

pub fn handler() -> SubCommandHandler {
    SubCommandHandler::new("init", execute)
}

/// /design init [path]
///
/// design.md を生成してプロジェクトを初期化する。
fn execute(args: &[String], _session: &mut AgentSession) -> Result<Output, CommandError> {
    let root = resolve_root(args.first().map(|s| s.as_str()));
    let design_path = design_file(&root);

    if design_path.exists() {
        return Ok(Output::text(format!(
            "design.md already exists at {}\nRun `design converge` to check for issues.",
            design_path.display()
        )));
    }

    let template = default_template();
    let yaml = design_to_yaml(&template);
    fs::write(&design_path, &yaml)
        .map_err(|e| CommandError::ExecutionError(format!("Cannot write design.md: {e}")))?;

    // Save baseline = initial template
    let (initial_version, _) = make_initial_version(template);
    save_baseline(&root, &initial_version).map_err(CommandError::ExecutionError)?;

    // Ensure history dir exists
    let hist = super::history_dir(&root);
    fs::create_dir_all(&hist)
        .map_err(|e| CommandError::ExecutionError(format!("Cannot create history dir: {e}")))?;

    Ok(Output::text(format!(
        "Created design.md at {}\nCreated .design/baseline.json\n\nEdit design.md, then run:\n  design converge",
        design_path.display()
    )))
}

fn default_template() -> DesignDocument {
    DesignDocument {
        stage: Stage::Context,
        context: Some(ContextSpec {
            target_user: Some("developer".to_string()),
            use_case: Some("build application".to_string()),
            environment: Some("local".to_string()),
        }),
        function: Some(FunctionSpec {
            functions: vec!["main".to_string()],
        }),
        architecture: Some(ArchitectureSpec {}),
        interface: Some(VersionInterfaceSpec {}),
        data: Some(DataSpec {}),
        execution: Some(ExecutionSpec {
            steps: vec!["setup".to_string(), "run".to_string()],
        }),
        metadata: Metadata::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::AgentSession;
    use std::fs;

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dbm_init_test_{name}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn init_creates_design_md() {
        let dir = tmp_dir("creates");
        let design_path = dir.join("design.md");
        // Ensure it doesn't exist first
        let _ = fs::remove_file(&design_path);
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&[dir.to_str().unwrap().to_string()], &mut session).unwrap();
        assert!(out.message.contains("Created design.md"));
        assert!(design_path.exists());
    }

    #[test]
    fn init_skips_if_already_exists() {
        let dir = tmp_dir("already_exists");
        fs::write(dir.join("design.md"), "stage: Context\n").unwrap();
        let h = handler();
        let mut session = AgentSession::new();
        let out = (h.execute)(&[dir.to_str().unwrap().to_string()], &mut session).unwrap();
        assert!(out.message.contains("already exists"));
    }
}
