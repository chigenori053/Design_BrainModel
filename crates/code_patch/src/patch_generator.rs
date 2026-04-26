use std::collections::BTreeMap;

use code_diff::{ChangeSet, replay_changes};
use code_ir::program_v1::{BackendLanguage, Program};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Patch {
    pub edits: Vec<TextEdit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextEdit {
    pub file: String,
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

pub fn generate_patch(old_ir: &Program, changes: &ChangeSet) -> Result<Patch, PatchError> {
    generate_patch_for_backend(old_ir, changes, BackendLanguage::Rust)
}

pub fn generate_patch_for_backend(
    old_ir: &Program,
    changes: &ChangeSet,
    backend: BackendLanguage,
) -> Result<Patch, PatchError> {
    let next = replay_changes(old_ir, changes).map_err(PatchError::Replay)?;
    let old_files = render_files(old_ir, backend.clone());
    let new_files = render_files(&next, backend);
    let mut file_names = old_files
        .keys()
        .chain(new_files.keys())
        .cloned()
        .collect::<Vec<_>>();
    file_names.sort();
    file_names.dedup();

    let mut edits = file_names
        .into_iter()
        .filter_map(|file| {
            let old = old_files.get(&file).cloned().unwrap_or_default();
            let new = new_files.get(&file).cloned().unwrap_or_default();
            if old == new {
                None
            } else {
                Some(TextEdit {
                    file,
                    start: 0,
                    end: old.len(),
                    replacement: new,
                })
            }
        })
        .collect::<Vec<_>>();
    edits.sort_by(|lhs, rhs| lhs.file.cmp(&rhs.file).then(lhs.start.cmp(&rhs.start)));
    Ok(Patch { edits })
}

fn render_files(program: &Program, backend: BackendLanguage) -> BTreeMap<String, String> {
    program
        .render_canonical_source_tree(backend)
        .into_iter()
        .collect::<BTreeMap<_, _>>()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchError {
    Replay(code_diff::ir_diff::ReplayError),
}

impl std::fmt::Display for PatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Replay(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for PatchError {}
