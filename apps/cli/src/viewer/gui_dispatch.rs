use std::path::Path;

use serde::Serialize;

use crate::refactor::GuiAction;

use super::{StructureViewIR, session::apply_session_action};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GuiCommandSpec {
    pub command_kind: String,
    pub target: String,
    pub node: Option<String>,
    pub session_id: String,
    pub stage: String,
}

pub fn dispatch_gui_action(
    root: &Path,
    event: GuiAction,
) -> Result<(GuiCommandSpec, StructureViewIR), String> {
    let mut event = event;
    if event.project_root.is_none() {
        event.project_root = Some(root.to_path_buf());
    }
    let stage = format!("{:?}", event.mode).to_lowercase();
    let (session, refreshed) = apply_session_action(root, event.clone())?;
    Ok((
        GuiCommandSpec {
            command_kind: "Refactor".to_string(),
            target: event.target,
            node: event.node,
            session_id: session.session_id,
            stage,
        },
        refreshed,
    ))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::refactor::GuiAction;

    use super::*;

    #[test]
    fn gui_dispatch_refreshes_ir_for_preview_flow() {
        let unique = format!(
            "gui_dispatch_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        fs::create_dir_all(root.join("src")).expect("create src");
        fs::write(
            root.join("Cargo.toml"),
            "[package]\nname = \"gui_dispatch\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("cargo");
        fs::write(
            root.join("src/lib.rs"),
            "pub mod renderer;\npub mod debug;\n",
        )
        .expect("lib");
        fs::write(
            root.join("src/renderer.rs"),
            "use crate::debug;\npub fn render() {}\n",
        )
        .expect("renderer");
        fs::write(
            root.join("src/debug.rs"),
            "use crate::renderer;\npub fn debug() {}\n",
        )
        .expect("debug");

        let (command, refreshed) = dispatch_gui_action(
            &root,
            GuiAction {
                action: "refactor".to_string(),
                target: "cycle".to_string(),
                node: Some("renderer".to_string()),
                project_root: Some(root.clone()),
                selected_nodes: Vec::new(),
                selected_edges: Vec::new(),
                mode: crate::refactor::GuiActionMode::Preview,
            },
        )
        .expect("dispatch");
        assert_eq!(command.command_kind, "Refactor");
        assert_eq!(command.stage, "preview");
        assert!(refreshed.preview.is_some());
    }
}
