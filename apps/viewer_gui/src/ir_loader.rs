use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use design_cli::viewer::StructureViewIR;

#[derive(Debug, Clone)]
pub struct IrSnapshot {
    pub ir: StructureViewIR,
    pub modified: Option<SystemTime>,
}

pub fn load_ir(path: &Path) -> Result<IrSnapshot, String> {
    let raw = fs::read_to_string(path)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    let ir = serde_json::from_str(&raw)
        .map_err(|err| format!("invalid IR JSON {}: {err}", path.display()))?;
    let modified = fs::metadata(path)
        .ok()
        .and_then(|meta| meta.modified().ok());
    Ok(IrSnapshot { ir, modified })
}

#[derive(Debug, Clone)]
pub struct IrTracker {
    path: PathBuf,
    modified: Option<SystemTime>,
}

impl IrTracker {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            modified: None,
        }
    }

    pub fn load_initial(&mut self) -> Result<StructureViewIR, String> {
        let snapshot = load_ir(&self.path)?;
        self.modified = snapshot.modified;
        Ok(snapshot.ir)
    }

    pub fn reload_if_changed(&mut self) -> Result<Option<StructureViewIR>, String> {
        let modified = fs::metadata(&self.path)
            .ok()
            .and_then(|meta| meta.modified().ok());
        if modified.is_some() && modified == self.modified {
            return Ok(None);
        }
        let snapshot = load_ir(&self.path)?;
        self.modified = snapshot.modified;
        Ok(Some(snapshot.ir))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use design_cli::viewer::{DesignSyncStatus, StructureViewIR, ViewerSelection};

    use super::*;

    #[test]
    fn load_ir_round_trips_json() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("viewer_gui_ir_{unique}"));
        fs::create_dir_all(&root).expect("create root");
        let path = root.join("structure_view.json");
        let ir = StructureViewIR {
            version: 2,
            nodes: Vec::new(),
            edges: Vec::new(),
            preview: None,
            snapshots: Vec::new(),
            history: Vec::new(),
            risk_overlay: Vec::new(),
            selection: ViewerSelection::default(),
            candidates: Vec::new(),
            heatmap: Vec::new(),
            design_sync: DesignSyncStatus::default(),
        };
        fs::write(&path, serde_json::to_string_pretty(&ir).expect("serialize")).expect("write");

        let snapshot = load_ir(&path).expect("load");
        assert_eq!(snapshot.ir.version, 2);
    }
}
