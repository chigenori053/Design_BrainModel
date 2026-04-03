use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::model::StructureViewIR;

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
