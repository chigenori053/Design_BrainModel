use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use design_cli::refactor::{GuiAction, GuiActionMode};

pub fn gui_action_path(root: &Path) -> PathBuf {
    root.join(".dbm").join("gui_action.json")
}

#[derive(Debug, Clone)]
pub struct ActionRequest {
    pub target: String,
    pub node: Option<String>,
    pub selected_nodes: Vec<String>,
    pub mode: GuiActionMode,
}

#[derive(Debug, Clone)]
pub struct DispatchResult {
    pub action_path: PathBuf,
    pub stdout: String,
}

pub fn write_action(root: &Path, request: &ActionRequest) -> Result<PathBuf, String> {
    let path = gui_action_path(root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    let event = GuiAction {
        action: "refactor".to_string(),
        target: request.target.clone(),
        node: request.node.clone(),
        project_root: Some(root.to_path_buf()),
        selected_nodes: request.selected_nodes.clone(),
        selected_edges: Vec::new(),
        mode: request.mode,
    };
    fs::write(
        &path,
        serde_json::to_string_pretty(&event).map_err(|err| err.to_string())?,
    )
    .map_err(|err| format!("failed to write {}: {err}", path.display()))?;
    Ok(path)
}

pub fn dispatch_action(
    cli_path: &Path,
    root: &Path,
    action_path: &Path,
) -> Result<DispatchResult, String> {
    let output = Command::new(cli_path)
        .arg("structure")
        .arg("dispatch")
        .arg(root)
        .arg("--event")
        .arg(action_path)
        .arg("--json")
        .output()
        .map_err(|err| format!("failed to launch {}: {err}", cli_path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "CLI dispatch failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(DispatchResult {
        action_path: action_path.to_path_buf(),
        stdout: String::from_utf8_lossy(&output.stdout).to_string(),
    })
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn write_action_emits_canonical_gui_action_file() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("viewer_gui_ipc_{unique}"));
        let path = write_action(
            &root,
            &ActionRequest {
                target: "cycle".to_string(),
                node: Some("renderer".to_string()),
                selected_nodes: vec!["renderer".to_string()],
                mode: GuiActionMode::Apply,
            },
        )
        .expect("write action");
        let raw = fs::read_to_string(&path).expect("read action");
        assert!(raw.contains("\"target\": \"cycle\""));
        assert!(path.ends_with(".dbm/gui_action.json"));
    }
}
