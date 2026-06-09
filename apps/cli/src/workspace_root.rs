use std::path::{Path, PathBuf};

pub use core_types::{WorkspaceMigrationReport, WorkspaceRoot};

use crate::global_holographic_memory::GlobalHolographicMemoryStore;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceVerification {
    pub root: PathBuf,
    pub root_found: bool,
    pub analyze_ok: bool,
    pub memory_ok: bool,
    pub logs_ok: bool,
    pub mutation_ok: bool,
    pub runtime_ok: bool,
}

pub fn initialize_workspace() -> Result<WorkspaceMigrationReport, String> {
    let report = WorkspaceRoot::migrate_legacy_layout().map_err(|err| err.to_string())?;
    GlobalHolographicMemoryStore::default_for_workspace(&report.canonical_root)
        .rebuild_index()
        .map_err(|err| format!("global memory index rebuild failed: {err}"))?;
    Ok(report)
}

pub fn verify_workspace(root: &Path) -> WorkspaceVerification {
    let dbm = root.join(".dbm");
    WorkspaceVerification {
        root: root.to_path_buf(),
        root_found: root.is_dir() && dbm.is_dir(),
        analyze_ok: dbm.join("analyze").is_dir(),
        memory_ok: dbm.join("memory").is_dir(),
        logs_ok: dbm.join("logs").is_dir(),
        mutation_ok: dbm.join("mutations").is_dir(),
        runtime_ok: dbm.join("runtime").is_dir(),
    }
}

pub fn render_workspace_verification(verification: &WorkspaceVerification) -> String {
    format!(
        "Workspace Verification\n\nRoot:\n{}\n{}\n\nAnalyze:\n{}\n\nMemory:\n{}\n\nLogs:\n{}\n\nMutation:\n{}\n\nRuntime:\n{}",
        verification.root.display(),
        status(verification.root_found),
        status(verification.analyze_ok),
        status(verification.memory_ok),
        status(verification.logs_ok),
        status(verification.mutation_ok),
        status(verification.runtime_ok),
    )
}

fn status(ok: bool) -> &'static str {
    if ok { "OK" } else { "MISSING" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn verification_accepts_canonical_layout() {
        let dir = tempfile::tempdir().expect("tempdir");
        WorkspaceRoot::ensure_layout(dir.path()).expect("layout");

        let verification = verify_workspace(dir.path());

        assert!(verification.root_found);
        assert!(verification.analyze_ok);
        assert!(verification.memory_ok);
        assert!(verification.logs_ok);
        assert!(verification.mutation_ok);
        assert!(verification.runtime_ok);
    }
}
