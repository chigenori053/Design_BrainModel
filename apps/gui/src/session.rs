use hybrid_vm::{
    ConceptUnitV2, HybridVM, MeaningLayerSnapshotV2, SemanticUnitL1V2,
};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct HistoryEntry {
    pub l1_units: Vec<SemanticUnitL1V2>,
    pub l2_units: Vec<ConceptUnitV2>,
    pub snapshot: MeaningLayerSnapshotV2,
}

pub struct History {
    entries: Vec<HistoryEntry>,
    cursor: usize,
}

impl History {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
        }
    }

    pub fn push(&mut self, entry: HistoryEntry) {
        if self.cursor < self.entries.len() {
            self.entries.truncate(self.cursor);
        }
        self.entries.push(entry);
        if self.entries.len() > 100 {
            self.entries.remove(0);
        }
        self.cursor = self.entries.len();
    }

    pub fn undo(&mut self) -> Option<&HistoryEntry> {
        if self.cursor > 1 {
            self.cursor -= 1;
            Some(&self.entries[self.cursor - 1])
        } else {
            None
        }
    }

    pub fn redo(&mut self) -> Option<&HistoryEntry> {
        if self.cursor < self.entries.len() {
            self.cursor += 1;
            Some(&self.entries[self.cursor - 1])
        } else {
            None
        }
    }
}

pub struct GuiSession {
    pub id: String,
    pub vm: HybridVM,
    #[allow(dead_code)]
    pub created_at: u64,
    pub last_modified: u64,
    pub history: History,
}

impl GuiSession {
    pub fn new(id: &str, store_path: std::path::PathBuf) -> Result<Self, std::io::Error> {
        let vm_store = store_path.join(format!("session_{id}")).join("vm");
        std::fs::create_dir_all(&vm_store)?;
        
        let vm = HybridVM::for_cli_storage(vm_store)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Ok(Self {
            id: id.to_string(),
            vm,
            created_at: now,
            last_modified: now,
            history: History::new(),
        })
    }

    pub fn update_modified(&mut self) {
        self.last_modified = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
    }
}
